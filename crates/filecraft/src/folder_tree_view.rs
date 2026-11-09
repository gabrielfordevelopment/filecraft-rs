//! The "Folders to include" card with the folder tree, shared by the export tabs.

use egui::{Rect, Sense, Ui};
use filecraft_core::folder_tree::{FolderTree, NodeId};

use crate::state::TreeTab;
use crate::strings;
use crate::theme;
use crate::widgets;

const TREE_INDENT: f32 = 20.0;
/// The height of a folder row, a little more than the check box.
const TREE_ROW_HEIGHT: f32 = 18.0;

enum TreeAction {
    Toggle(NodeId),
    SetExpanded(NodeId, bool),
}

/// What the user asked for in the folders card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoldersAction {
    ToggleFullscreen,
    ChooseSource,
}

/// Draws the folders card in `rect`. Tabs linked to this tree are shown with a link icon in the
/// header. Without a tree it asks for a source folder.
pub fn folders_group(
    ui: &mut Ui,
    rect: Rect,
    id_salt: &str,
    tree: Option<&mut FolderTree>,
    linked: &[TreeTab],
    fullscreen_active: bool,
) -> Option<FoldersAction> {
    let count = tree.as_deref().map_or(0, FolderTree::selected_count);
    let mut toggle = false;
    let mut choose_source = false;
    widgets::card_in(
        ui,
        rect,
        true,
        |ui| {
            widgets::card_title(ui, strings::COMMON_FOLDERS_HEADER);
            ui.add_space(6.0);
            widgets::count_badge(ui, &count.to_string());
            if !linked.is_empty() {
                ui.add_space(5.0);
                widgets::glyph(
                    ui,
                    crate::icons::LINK.glyph,
                    20.0,
                    crate::icons::LINK.color(),
                );
                ui.add_space(2.0);
                for tab in linked {
                    ui.add_space(2.0);
                    let icon = tab.icon();
                    widgets::glyph(ui, icon.glyph, 20.0, icon.color()).on_hover_text(tab.title());
                    ui.add_space(2.0);
                }
            }
        },
        |ui| toggle = widgets::fullscreen_header_toggle(ui, fullscreen_active),
        |ui| {
            let Some(tree) = tree else {
                choose_source = empty_state(ui);
                return;
            };
            let mut actions = Vec::new();
            egui::ScrollArea::both()
                .id_salt(id_salt)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // Each row is a horizontal layout, which is as high as `interact_size.y`.
                    ui.spacing_mut().interact_size.y = TREE_ROW_HEIGHT;
                    folder_row(ui, tree, FolderTree::ROOT, &mut actions);
                });
            for action in actions {
                match action {
                    TreeAction::Toggle(id) => tree.toggle(id),
                    TreeAction::SetExpanded(id, expanded) => tree.set_expanded(id, expanded),
                }
            }
        },
    );
    if toggle {
        Some(FoldersAction::ToggleFullscreen)
    } else {
        choose_source.then_some(FoldersAction::ChooseSource)
    }
}

/// A hint and a button to choose the source folder, centered in the group box. Returns true when
/// the button was clicked.
fn empty_state(ui: &mut Ui) -> bool {
    let colors = theme::palette();
    let body = ui.max_rect();
    let message = ui.painter().layout(
        strings::FOLDERS_EMPTY_MESSAGE.to_owned(),
        theme::body(theme::BODY_SIZE),
        colors.text_muted,
        (body.width() - 20.0).max(1.0),
    );
    let button_width = widgets::button_width(ui, strings::CHOOSE_SOURCE_FOLDER_BUTTON);
    let height = message.size().y + 10.0 + widgets::control_height(ui);
    let top = body.center().y - height / 2.0;
    let message_pos = egui::pos2(body.center().x - message.size().x / 2.0, top);
    ui.painter()
        .galley(message_pos, message.clone(), colors.text_muted);
    let button_rect = Rect::from_min_size(
        egui::pos2(
            body.center().x - button_width / 2.0,
            top + message.size().y + 10.0,
        ),
        egui::vec2(button_width, widgets::control_height(ui)),
    );
    let mut button_ui = ui.new_child(egui::UiBuilder::new().max_rect(button_rect));
    widgets::button(
        &mut button_ui,
        strings::CHOOSE_SOURCE_FOLDER_BUTTON,
        widgets::ButtonKind::Secondary,
        true,
        egui::Vec2::ZERO,
    )
    .clicked()
}

fn folder_row(ui: &mut Ui, tree: &FolderTree, id: NodeId, actions: &mut Vec<TreeAction>) {
    let node = tree.node(id);
    ui.horizontal(|ui| {
        let expander = widgets::expander(
            ui,
            ui.scope_id().with(("folder", id)),
            node.expanded,
            tree.has_children(id),
        );
        if expander.clicked() && tree.has_children(id) {
            actions.push(TreeAction::SetExpanded(id, !node.expanded));
        }
        if widgets::check_box(ui, node.state).clicked() {
            actions.push(TreeAction::Toggle(id));
        }
        ui.add_space(5.0);
        let name = widgets::label(
            ui,
            &node.name,
            theme::body(theme::BODY_SIZE),
            theme::palette().text,
        )
        .interact(Sense::click());
        let name = match &node.load_error {
            Some(error) => name.on_hover_text(error),
            None => name,
        };
        if name.double_clicked() && tree.has_children(id) {
            actions.push(TreeAction::SetExpanded(id, !node.expanded));
        }
    });
    if node.expanded && !tree.children(id).is_empty() {
        ui.horizontal(|ui| {
            ui.add_space(TREE_INDENT);
            ui.vertical(|ui| {
                for child in tree.children(id) {
                    folder_row(ui, tree, *child, actions);
                }
            });
        });
    }
}
