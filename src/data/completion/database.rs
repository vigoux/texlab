use crate::client::LspClient;
use crate::data::completion::component::LatexComponent;
use crate::data::completion::dependency::LatexDependency;
use crate::syntax::SyntaxTree;
use crate::tex::resolver::TexResolver;
use crate::workspace::Document;
use futures::channel::mpsc;
use futures::compat::*;
use futures::lock::Mutex;
use futures::prelude::*;
use itertools::Itertools;
use lsp_types::*;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct LatexComponentDatabase {
    pub components: Vec<Arc<LatexComponent>>,
}

impl LatexComponentDatabase {
    pub fn related_components(&self, documents: &[Arc<Document>]) -> Vec<Arc<LatexComponent>> {
        let mut start_components = Vec::new();
        for document in documents {
            if let SyntaxTree::Latex(tree) = &document.tree {
                tree.components
                    .iter()
                    .flat_map(|file| self.find(&file.into()))
                    .for_each(|component| start_components.push(component))
            }
        }

        let mut all_components = Vec::new();
        for component in start_components {
            all_components.push(Arc::clone(&component));
            component
                .references
                .iter()
                .flat_map(|file| self.find(&file))
                .for_each(|component| all_components.push(component))
        }

        all_components
            .iter()
            .unique_by(|component| &component.file_names)
            .map(Arc::clone)
            .collect()
    }

    fn find(&self, name: &String) -> Option<Arc<LatexComponent>> {
        self.components
            .iter()
            .find(|component| component.file_names.contains(name))
            .map(Arc::clone)
    }
}

#[derive(Debug)]
pub struct LatexComponentDatabaseManager<C> {
    database_path: PathBuf,
    components_by_name: Mutex<HashMap<String, Arc<LatexComponent>>>,
    sender: Mutex<mpsc::Sender<PathBuf>>,
    receiver: Mutex<mpsc::Receiver<PathBuf>>,
    resolver: Arc<TexResolver>,
    client: Arc<C>,
}

impl<C> LatexComponentDatabaseManager<C>
where
    C: LspClient + Send + Sync + 'static,
{
    pub fn new<P: AsRef<Path>>(
        database_path: P,
        database: LatexComponentDatabase,
        resolver: Arc<TexResolver>,
        client: Arc<C>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(0);
        let mut components_by_name = HashMap::new();
        for component in database.components {
            for file_name in &component.file_names {
                components_by_name.insert(file_name.to_owned(), Arc::clone(&component));
            }
        }

        Self {
            database_path: database_path.as_ref().to_owned(),
            components_by_name: Mutex::new(components_by_name),
            sender: Mutex::new(sender),
            receiver: Mutex::new(receiver),
            resolver,
            client,
        }
    }

    pub fn load_or_create<P: AsRef<Path>>(
        path: P,
        resolver: Arc<TexResolver>,
        client: Arc<C>,
    ) -> Self {
        let database = match fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
        {
            Some(components) => LatexComponentDatabase { components },
            None => LatexComponentDatabase::default(),
        };

        Self::new(path, database, resolver, client)
    }

    pub async fn get(&self) -> LatexComponentDatabase {
        let components: Vec<_> = self
            .components_by_name
            .lock()
            .await
            .values()
            .map(Arc::clone)
            .collect();
        LatexComponentDatabase { components }
    }

    pub async fn enqueue<'a>(&'a self, file_name: &'a str) {
        if { !self.components_by_name.lock().await.contains_key(file_name) } {
            if let Some(file) = self.resolver.files_by_name.get(file_name) {
                let mut sender = self.sender.lock().await;
                sender.send(file.to_owned()).await.unwrap();
            }
        }
    }

    pub async fn close(&self) {
        self.sender.lock().await.disconnect();
    }

    pub async fn listen(&self) {
        while let Some(file) = {
            let mut receiver = self.receiver.lock().await;
            receiver.next().await
        } {
            let file_name = file.file_name().unwrap().to_str().unwrap();
            if { self.components_by_name.lock().await.contains_key(file_name) } {
                continue;
            }

            self.analyze(file).await;
        }
    }

    async fn analyze(&self, file: PathBuf) {
        let progress_id = "index";
        let params = ProgressStartParams {
            id: progress_id.into(),
            title: "Indexing".into(),
            cancellable: Some(false),
            message: Some(
                file.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
                    .into(),
            ),
            percentage: None,
        };
        self.client.progress_start(params).await;

        let components = LatexDependency::load(&file, &self.resolver)
            .await
            .into_components(&self.resolver, &self.components_by_name)
            .await;

        for component in components {
            let dependency = &component[0];
            let params = ProgressReportParams {
                id: progress_id.into(),
                message: Some(
                    dependency
                        .file
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned()
                        .into(),
                ),
                percentage: None,
            };
            self.client.progress_report(params).await;

            let mut loaded_refs = Vec::new();
            for reference in dependency.references() {
                let file_name = reference.file_name().unwrap().to_str().unwrap();
                if let Some(component) = { self.components_by_name.lock().await.get(file_name) } {
                    loaded_refs.push(Arc::clone(&component));
                }
            }

            let component = match LatexComponent::load(&component, loaded_refs).await {
                Some(component) => component,
                None => {
                    let file_name = file.file_name().unwrap().to_str().unwrap().to_owned();
                    log::warn!("Component `{}` could not be analyzed", &file_name);

                    LatexComponent {
                        file_names: vec![file_name],
                        references: dependency
                            .references()
                            .map(|ref_| ref_.to_str().unwrap().to_owned())
                            .collect(),
                        commands: Vec::new(),
                        environments: Vec::new(),
                    }
                }
            };

            let component = Arc::new(component);
            for file_name in &component.file_names {
                let component = Arc::clone(&component);
                self.components_by_name
                    .lock()
                    .await
                    .insert(file_name.to_owned(), component);
            }
        }

        let params = ProgressDoneParams {
            id: progress_id.into(),
        };
        self.client.progress_done(params).await;
        self.save_database().await;
    }

    async fn save_database(&self) {
        let database = self.get().await;
        let json = serde_json::to_string(&database.components).unwrap();

        if let Err(why) = tokio::fs::write(self.database_path.to_owned(), json)
            .compat()
            .await
        {
            log::error!("Unable to save component database: {}", why);
        }
    }
}
