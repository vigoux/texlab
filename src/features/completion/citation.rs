use std::sync::Arc;

use lsp_types::CompletionParams;
use once_cell::sync::Lazy;
use regex::Regex;
use rowan::{ast::AstNode, TextRange};

use crate::{
    features::{cursor::CursorContext, lsp_kinds::Structure},
    syntax::{
        bibtex::{self, HasName, HasType},
        latex,
    },
    BibtexEntryTypeCategory, Document, LANGUAGE_DATA,
};

use super::types::{InternalCompletionItem, InternalCompletionItemData};

pub fn complete_citations<'a>(
    context: &'a CursorContext<CompletionParams>,
    items: &mut Vec<InternalCompletionItem<'a>>,
) -> Option<()> {
    let token = context.cursor.as_latex()?;

    let range = if token.kind() == latex::WORD {
        latex::Key::cast(token.parent()?)
            .map(|key| latex::small_range(&key))
            .or_else(|| {
                token
                    .parent()
                    .and_then(latex::Text::cast)
                    .map(|text| latex::small_range(&text))
            })?
    } else {
        TextRange::empty(context.offset)
    };

    check_citation(context).or_else(|| check_acronym(context))?;
    for document in context.request.workspace.documents_by_uri.values() {
        if let Some(data) = document.data.as_bibtex() {
            for entry in bibtex::SyntaxNode::new_root(data.green.clone())
                .children()
                .filter_map(bibtex::Entry::cast)
            {
                if let Some(item) = make_item(document, &entry, range) {
                    items.push(item);
                }
            }
        }
    }

    Some(())
}

fn check_citation(context: &CursorContext<CompletionParams>) -> Option<()> {
    let (_, _, group) = context.find_curly_group_word_list()?;
    latex::Citation::cast(group.syntax().parent()?)?;
    Some(())
}

fn check_acronym(context: &CursorContext<CompletionParams>) -> Option<()> {
    let token = context.cursor.as_latex()?;

    let pair = token
        .parent_ancestors()
        .find_map(latex::KeyValuePair::cast)?;
    if pair.key()?.to_string() != "cite" {
        return None;
    }

    latex::AcronymDeclaration::cast(pair.syntax().parent()?.parent()?.parent()?)?;
    Some(())
}

fn make_item<'a>(
    document: &'a Document,
    entry: &bibtex::Entry,
    range: TextRange,
) -> Option<InternalCompletionItem<'a>> {
    let key = entry.name_token()?.to_string();
    let ty = LANGUAGE_DATA
        .find_entry_type(&entry.type_token()?.text()[1..])
        .map_or_else(
            || Structure::Entry(BibtexEntryTypeCategory::Misc),
            |ty| Structure::Entry(ty.category),
        );

    let entry_code = entry.syntax().text().to_string();
    let text = format!(
        "{} {}",
        key,
        WHITESPACE_REGEX
            .replace_all(
                &entry_code
                    .replace('{', " ")
                    .replace('}', " ")
                    .replace(',', " ")
                    .replace('=', " "),
                " "
            )
            .trim(),
    );

    Some(InternalCompletionItem::new(
        range,
        InternalCompletionItemData::Citation {
            uri: Arc::clone(&document.uri),
            key,
            text,
            ty,
        },
    ))
}

static WHITESPACE_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new("\\s+").unwrap());
