use collections::HashSet;
use editor::{Editor, GoToDiagnostic};
use gpui::{
    div, Div, EventEmitter, InteractiveComponent, ParentComponent, Render, Stateful,
    StatefulInteractiveComponent, Styled, Subscription, View, ViewContext, WeakView,
};
use language::Diagnostic;
use lsp::LanguageServerId;
use theme::ActiveTheme;
use ui::{h_stack, Icon, IconElement, Label, TextColor, Tooltip};
use workspace::{item::ItemHandle, StatusItemView, ToolbarItemEvent, Workspace};

use crate::ProjectDiagnosticsEditor;

pub struct DiagnosticIndicator {
    summary: project::DiagnosticSummary,
    active_editor: Option<WeakView<Editor>>,
    workspace: WeakView<Workspace>,
    current_diagnostic: Option<Diagnostic>,
    in_progress_checks: HashSet<LanguageServerId>,
    _observe_active_editor: Option<Subscription>,
}

impl Render for DiagnosticIndicator {
    type Element = Stateful<Self, Div<Self>>;

    fn render(&mut self, cx: &mut ViewContext<Self>) -> Self::Element {
        let mut summary_row = h_stack()
            .id(cx.entity_id())
            .on_action(Self::go_to_next_diagnostic)
            .rounded_md()
            .p_1()
            .cursor_pointer()
            .bg(gpui::green())
            .hover(|style| style.bg(cx.theme().colors().element_hover))
            .active(|style| style.bg(cx.theme().colors().element_active))
            .tooltip(|_, cx| Tooltip::text("Project Diagnostics", cx))
            .on_click(|this, _, cx| {
                if let Some(workspace) = this.workspace.upgrade() {
                    workspace.update(cx, |workspace, cx| {
                        ProjectDiagnosticsEditor::deploy(workspace, &Default::default(), cx)
                    })
                }
            });

        if self.summary.error_count > 0 {
            summary_row = summary_row.child(
                div()
                    .child(IconElement::new(Icon::XCircle).color(TextColor::Error))
                    .bg(gpui::red()),
            );
            summary_row = summary_row.child(
                div()
                    .child(Label::new(self.summary.error_count.to_string()))
                    .bg(gpui::yellow()),
            );
        }

        if self.summary.warning_count > 0 {
            summary_row = summary_row
                .child(IconElement::new(Icon::ExclamationTriangle).color(TextColor::Warning));
            summary_row = summary_row.child(Label::new(self.summary.warning_count.to_string()));
        }

        if self.summary.error_count == 0 && self.summary.warning_count == 0 {
            summary_row =
                summary_row.child(IconElement::new(Icon::Check).color(TextColor::Success));
        }

        summary_row
    }
}

impl DiagnosticIndicator {
    pub fn new(workspace: &Workspace, cx: &mut ViewContext<Self>) -> Self {
        let project = workspace.project();
        cx.subscribe(project, |this, project, event, cx| match event {
            project::Event::DiskBasedDiagnosticsStarted { language_server_id } => {
                this.in_progress_checks.insert(*language_server_id);
                cx.notify();
            }

            project::Event::DiskBasedDiagnosticsFinished { language_server_id }
            | project::Event::LanguageServerRemoved(language_server_id) => {
                this.summary = project.read(cx).diagnostic_summary(cx);
                this.in_progress_checks.remove(language_server_id);
                cx.notify();
            }

            project::Event::DiagnosticsUpdated { .. } => {
                this.summary = project.read(cx).diagnostic_summary(cx);
                cx.notify();
            }

            _ => {}
        })
        .detach();

        Self {
            summary: project.read(cx).diagnostic_summary(cx),
            in_progress_checks: project
                .read(cx)
                .language_servers_running_disk_based_diagnostics()
                .collect(),
            active_editor: None,
            workspace: workspace.weak_handle(),
            current_diagnostic: None,
            _observe_active_editor: None,
        }
    }

    fn go_to_next_diagnostic(&mut self, _: &GoToDiagnostic, cx: &mut ViewContext<Self>) {
        if let Some(editor) = self.active_editor.as_ref().and_then(|e| e.upgrade()) {
            editor.update(cx, |editor, cx| {
                editor.go_to_diagnostic_impl(editor::Direction::Next, cx);
            })
        }
    }

    fn update(&mut self, editor: View<Editor>, cx: &mut ViewContext<Self>) {
        let editor = editor.read(cx);
        let buffer = editor.buffer().read(cx);
        let cursor_position = editor.selections.newest::<usize>(cx).head();
        let new_diagnostic = buffer
            .snapshot(cx)
            .diagnostics_in_range::<_, usize>(cursor_position..cursor_position, false)
            .filter(|entry| !entry.range.is_empty())
            .min_by_key(|entry| (entry.diagnostic.severity, entry.range.len()))
            .map(|entry| entry.diagnostic);
        if new_diagnostic != self.current_diagnostic {
            self.current_diagnostic = new_diagnostic;
            cx.notify();
        }
    }
}

impl EventEmitter<ToolbarItemEvent> for DiagnosticIndicator {}

impl StatusItemView for DiagnosticIndicator {
    fn set_active_pane_item(
        &mut self,
        active_pane_item: Option<&dyn ItemHandle>,
        cx: &mut ViewContext<Self>,
    ) {
        if let Some(editor) = active_pane_item.and_then(|item| item.downcast::<Editor>()) {
            self.active_editor = Some(editor.downgrade());
            self._observe_active_editor = Some(cx.observe(&editor, Self::update));
            self.update(editor, cx);
        } else {
            self.active_editor = None;
            self.current_diagnostic = None;
            self._observe_active_editor = None;
        }
        cx.notify();
    }
}