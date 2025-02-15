use lsp_types::CompletionParams;

use crate::features::cursor::CursorContext;

use super::types::{InternalCompletionItem, InternalCompletionItemData};

pub fn complete_begin_command(
    context: &CursorContext<CompletionParams>,
    items: &mut Vec<InternalCompletionItem>,
) -> Option<()> {
    let range = context.cursor.command_range(context.offset)?;

    items.push(InternalCompletionItem::new(
        range,
        InternalCompletionItemData::BeginCommand,
    ));
    Some(())
}
