use crate::completion::factory;
use crate::completion::factory::LatexComponentId;
use crate::completion::latex::combinators::{self, Parameter};
use crate::data::language::language_data;
use crate::feature::{FeatureProvider, FeatureRequest};
use futures_boxed::boxed;
use lsp_types::{CompletionItem, CompletionParams, TextEdit};
use std::borrow::Cow;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct LatexPgfLibraryCompletionProvider;

impl FeatureProvider for LatexPgfLibraryCompletionProvider {
    type Params = CompletionParams;
    type Output = Vec<CompletionItem>;

    #[boxed]
    async fn execute<'a>(&'a self, request: &'a FeatureRequest<Self::Params>) -> Self::Output {
        let parameter = Parameter::new("\\usepgflibrary", 0);
        combinators::argument(
            request,
            std::iter::once(parameter),
            async move |_, name_range| {
                let mut items = Vec::new();
                for name in &language_data().pgf_libraries {
                    let text_edit = TextEdit::new(name_range, Cow::from(name));
                    let item = factory::pgf_library(request, name, text_edit);
                    items.push(item);
                }
                items
            },
        )
        .await
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct LatexTikzLibraryCompletionProvider;

impl FeatureProvider for LatexTikzLibraryCompletionProvider {
    type Params = CompletionParams;
    type Output = Vec<CompletionItem>;

    #[boxed]
    async fn execute<'a>(&'a self, request: &'a FeatureRequest<Self::Params>) -> Self::Output {
        let parameter = Parameter::new("\\usetikzlibrary", 0);
        combinators::argument(
            request,
            std::iter::once(parameter),
            async move |_, name_range| {
                let mut items = Vec::new();
                for name in &language_data().tikz_libraries {
                    let text_edit = TextEdit::new(name_range, Cow::from(name));
                    let item = factory::tikz_library(request, name, text_edit);
                    items.push(item);
                }
                items
            },
        )
        .await
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct LatexTikzCommandCompletionProvider;

impl FeatureProvider for LatexTikzCommandCompletionProvider {
    type Params = CompletionParams;
    type Output = Vec<CompletionItem>;

    #[boxed]
    async fn execute<'a>(&'a self, request: &'a FeatureRequest<Self::Params>) -> Self::Output {
        combinators::command(request, async move |command| {
            let component = LatexComponentId::Component(vec!["tikz.sty"]);
            let mut items = Vec::new();
            if request
                .component_database
                .related_components(request.related_documents())
                .iter()
                .any(|component| component.file_names.iter().any(|file| file == "tikz.sty"))
            {
                for name in &language_data().tikz_commands {
                    let text_edit = TextEdit::new(command.short_name_range(), Cow::from(name));
                    let item = factory::command(request, Cow::from(name), text_edit, &component);
                    items.push(item);
                }
            }
            items
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feature::{test_feature, FeatureSpec};
    use lsp_types::Position;

    #[test]
    fn test_pgf_library() {
        let items = test_feature(
            LatexPgfLibraryCompletionProvider,
            FeatureSpec {
                files: vec![FeatureSpec::file("foo.tex", "\\usepgflibrary{}")],
                main_file: "foo.tex",
                position: Position::new(0, 15),
                ..FeatureSpec::default()
            },
        );
        assert!(!items.is_empty());
    }

    #[test]
    fn test_tikz_library() {
        let items = test_feature(
            LatexTikzLibraryCompletionProvider,
            FeatureSpec {
                files: vec![FeatureSpec::file("foo.tex", "\\usetikzlibrary{}")],
                main_file: "foo.tex",
                position: Position::new(0, 16),
                ..FeatureSpec::default()
            },
        );
        assert!(!items.is_empty());
    }
}
