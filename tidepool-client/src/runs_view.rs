//! The runs pane: the activity feed, one live run detail and the dispatch form.
//!
//! The view is deliberately dumb. [`crate::runs::RunsController`] owns the
//! pool; this module takes a [`RunsSnapshot`] and turns it into rows. That keeps
//! the rendering testable without a socket and keeps a click from reaching the
//! network from inside a `Render` implementation.
//!
//! # The machine's own work says so
//!
//! [`ActivityRow::for_entry`] marks a run this client has seen running on this
//! machine, and [`StepRow::for_step`] marks each step's place. The words are on
//! the row (`This machine`, `Another machine (host)`), so the distinction
//! survives the palette being removed.
//!
//! # A failure is a sentence with the code beside it
//!
//! [`StepRow`] carries [`crate::runs::FailureExplanation::line`]: the sentence a
//! person reads, followed by the pool's code in parentheses. A host that was
//! offline reads as `The host was offline …` and still names
//! `daemon_not_connected`.
//!
//! # Following stops with the run
//!
//! The detail shows the run's own status and every step's, in position order.
//! A run whose status and steps are all terminal is drawn as finished; the
//! controller stops reading it, and the view shows no `following` hint.

use std::collections::BTreeSet;

use gpui::{Context, FocusHandle, Focusable, Render, SharedString, Window, div, prelude::*, px};

use gpui_kit::component::ActiveTheme;

use crate::runs::{
    ActivityEntry, ActivityFilters, DispatchForm, FailureExplanation, RunDetail, RunPlace,
    RunStepView, RunView, RunsSnapshot, TAIL_LINES, entry_run_id, entry_title, failure_explanation,
    kind_word, output_tail, place_of, run_failure_explanation, status_word,
};

/// One activity row, with its kind, its status words and where it happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityRow {
    /// The record's public id.
    pub id: String,
    /// The kind's word: `Arrival`, `Run`, `Step` or `Execution`.
    pub kind: String,
    /// A short heading for the record.
    pub title: String,
    /// The status in words.
    pub status: String,
    /// The project's public id, when the record names one.
    pub project: String,
    /// Whether this run has been seen running on this machine.
    pub local: bool,
    /// The run this row can open, when it names one.
    pub run: Option<String>,
}

impl ActivityRow {
    /// Build one feed row from an activity entry.
    ///
    /// `local_runs` is the set of run ids this client has seen running on this
    /// machine; a row for one of them is marked so.
    pub fn for_entry(entry: &ActivityEntry, local_runs: &BTreeSet<String>) -> Self {
        let run = entry_run_id(entry).map(str::to_owned);
        Self {
            id: entry.id.clone(),
            kind: kind_word(&entry.kind),
            title: entry_title(entry),
            status: status_word(&entry.status),
            project: entry.project.clone().unwrap_or_default(),
            local: run.as_deref().is_some_and(|run| local_runs.contains(run)),
            run,
        }
    }

    /// The whole row as one line of text.
    pub fn line(&self) -> String {
        let mut line = format!("{} · {} · {}", self.kind, self.title, self.status);
        if !self.project.is_empty() {
            line.push_str(&format!(" · {}", self.project));
        }
        if self.local {
            line.push_str(" · This machine");
        }
        line
    }

    /// The place words shown beside the row, when it is this machine's work.
    pub fn place_label(&self) -> Option<&'static str> {
        self.local.then_some("This machine")
    }
}

/// One step of the open run, ready to render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepRow {
    /// The step's authored id.
    pub step: String,
    /// Its position in the workflow.
    pub position: i64,
    /// The status in words.
    pub status: String,
    /// The step's place: this machine, another host, or unrecorded.
    pub place: String,
    /// Whether this step runs on this machine.
    pub local: bool,
    /// The trailing output lines, newest last.
    pub output: Vec<String>,
    /// The failure as words with its code, when the step failed.
    pub failure: Option<String>,
}

impl StepRow {
    /// Build one step row for the open run.
    pub fn for_step(step: &RunStepView, local_host: Option<&str>) -> Self {
        let place = place_of_step(step, local_host);
        Self {
            step: step.step.clone(),
            position: step.position,
            status: status_word(&step.status),
            local: place.is_local(),
            place: place.label(),
            output: output_tail(step.output.as_deref(), TAIL_LINES),
            failure: failure_explanation(step.error.as_deref(), step.reason.as_deref())
                .map(|failure| failure.line()),
        }
    }

    /// The whole row as one line of text.
    pub fn line(&self) -> String {
        let mut line = format!(
            "{}. {} · {} · {}",
            self.position, self.step, self.status, self.place
        );
        if let Some(failure) = self.failure.as_deref() {
            line.push_str(&format!(" · {failure}"));
        }
        line
    }
}

/// Where one step runs, given this machine's host.
fn place_of_step(step: &RunStepView, local_host: Option<&str>) -> RunPlace {
    let run = RunView {
        steps: vec![step.clone()],
        ..RunView::default()
    };
    place_of(&run, local_host)
}

/// The open run's heading: its trigger, status and place.
pub fn detail_heading(detail: &RunDetail, local_host: Option<&str>) -> String {
    let run = &detail.run;
    let trigger = if run.trigger_name.trim().is_empty() {
        run.trigger.trim()
    } else {
        run.trigger_name.trim()
    };
    let trigger = if trigger.is_empty() { "A run" } else { trigger };
    let place = place_of(run, local_host).label();
    let mut line = format!(
        "Run of '{trigger}' — {} — {place}",
        status_word(&run.status)
    );
    if detail.following {
        line.push_str(" · following");
    }
    line
}

/// Every step row of the open run, in position order.
pub fn detail_rows(detail: &RunDetail, local_host: Option<&str>) -> Vec<StepRow> {
    let mut steps: Vec<&RunStepView> = detail.run.steps.iter().collect();
    steps.sort_by_key(|step| step.position);
    steps
        .into_iter()
        .map(|step| StepRow::for_step(step, local_host))
        .collect()
}

/// The first failure a person should read for the open run, with its code.
pub fn detail_failure(detail: &RunDetail) -> Option<FailureExplanation> {
    run_failure_explanation(&detail.run)
}

/// The filter combination as the pane's header line.
pub fn filter_line(filters: &ActivityFilters) -> String {
    filters.summary()
}

/// The dispatch form's field labels and values, in tab order.
pub fn dispatch_lines(form: &DispatchForm) -> Vec<(String, String)> {
    vec![
        ("project".to_owned(), form.project().to_owned()),
        ("trigger".to_owned(), form.trigger().to_owned()),
        ("actor".to_owned(), form.actor().to_owned()),
        ("delivery key".to_owned(), form.delivery_key().to_owned()),
        ("input".to_owned(), form.input().to_owned()),
        ("revision".to_owned(), form.revision().to_owned()),
    ]
}

/// The runs pane.
pub struct RunsView {
    snapshot: RunsSnapshot,
    focus_handle: FocusHandle,
}

impl RunsView {
    /// Build the pane over the controller's last snapshot.
    pub fn new(snapshot: RunsSnapshot, cx: &mut Context<Self>) -> Self {
        Self {
            snapshot,
            focus_handle: cx.focus_handle(),
        }
    }

    /// The state this pane is drawing.
    pub fn snapshot(&self) -> &RunsSnapshot {
        &self.snapshot
    }

    /// Replace the state, for the next frame.
    pub fn set_snapshot(&mut self, snapshot: RunsSnapshot) {
        self.snapshot = snapshot;
    }

    /// The visible activity rows.
    pub fn rows(&self) -> Vec<ActivityRow> {
        self.snapshot
            .entries
            .iter()
            .map(|entry| ActivityRow::for_entry(entry, &self.snapshot.local_runs))
            .collect()
    }

    /// The header line for the filters.
    pub fn filter_line(&self) -> String {
        filter_line(&self.snapshot.filters)
    }

    /// The open run's heading, when one is open.
    pub fn heading(&self) -> Option<String> {
        self.snapshot
            .detail
            .as_ref()
            .map(|detail| detail_heading(detail, self.snapshot.local_host.as_deref()))
    }

    /// The open run's step rows, when one is open.
    pub fn detail_rows(&self) -> Vec<StepRow> {
        self.snapshot
            .detail
            .as_ref()
            .map(|detail| detail_rows(detail, self.snapshot.local_host.as_deref()))
            .unwrap_or_default()
    }

    /// The open run's leading failure, when it has one.
    pub fn failure_line(&self) -> Option<String> {
        self.snapshot
            .detail
            .as_ref()
            .and_then(detail_failure)
            .map(|failure| failure.line())
    }

    /// The dispatch form's fields, while one is open.
    pub fn dispatch_fields(&self) -> Vec<(String, String)> {
        self.snapshot
            .dispatch
            .as_ref()
            .map(dispatch_lines)
            .unwrap_or_default()
    }

    /// The last problem worth telling the person.
    pub fn notice(&self) -> Option<&str> {
        self.snapshot.notice.as_deref()
    }

    fn activity_list(&self, muted: gpui::Hsla) -> impl IntoElement {
        let mut list = div().flex().flex_col().gap_2();
        for row in self.rows() {
            let status = row.status.clone();
            let mut line = div()
                .flex()
                .flex_row()
                .justify_between()
                .gap_2()
                .text_sm()
                .child(div().child(row.line()));
            if let Some(place) = row.place_label() {
                line = line.child(div().text_xs().text_color(muted).child(place));
            } else {
                line = line.child(div().text_xs().text_color(muted).child(status));
            }
            list = list.child(line);
        }
        list
    }

    fn run_detail(&self, muted: gpui::Hsla) -> impl IntoElement {
        let mut column = div().flex().flex_col().gap_2();
        if let Some(heading) = self.heading() {
            column = column.child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(heading),
            );
        }
        for row in self.detail_rows() {
            let mut step = div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .justify_between()
                        .gap_2()
                        .child(div().text_sm().child(row.line())),
                )
                .child(div().text_xs().text_color(muted).child(row.place.clone()));
            for output in row.output {
                step = step.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(SharedString::from(output)),
                );
            }
            column = column.child(step);
        }
        if let Some(failure) = self.failure_line() {
            column = column.child(div().text_sm().child(failure));
        }
        column
    }

    fn dispatch_form(&self, muted: gpui::Hsla) -> impl IntoElement {
        let mut form = div().flex().flex_col().gap_1();
        for (label, value) in self.dispatch_fields() {
            form = form.child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .text_xs()
                    .child(div().w(px(72.0)).text_color(muted).child(label))
                    .child(div().child(value)),
            );
        }
        form
    }
}

impl Focusable for RunsView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for RunsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let foreground = cx.theme().foreground;

        let filters = SharedString::from(self.filter_line());
        let mut column = div()
            .flex()
            .flex_col()
            .size_full()
            .p_4()
            .gap_3()
            .text_color(foreground)
            .child(
                div()
                    .text_lg()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child("Activity"),
            )
            .child(div().text_xs().text_color(muted).child(filters))
            .child(div().h(px(1.0)).bg(border))
            .child(self.activity_list(muted));

        if self.snapshot.detail.is_some() {
            column = column.child(div().h(px(1.0)).bg(border));
            column = column.child(self.run_detail(muted));
        }
        if self.snapshot.dispatch.is_some() {
            column = column.child(div().h(px(1.0)).bg(border));
            column = column.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_sm().child("Dispatch a run"))
                    .child(self.dispatch_form(muted)),
            );
        }
        if let Some(notice) = self.notice().map(SharedString::from) {
            column = column.child(div().text_sm().text_color(muted).child(notice));
        }
        column
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runs::{RunDetail, RunStepView, RunView};

    fn entry(id: &str, project: &str) -> ActivityEntry {
        ActivityEntry {
            id: id.to_owned(),
            kind: "run".to_owned(),
            status: "running".to_owned(),
            project: Some(project.to_owned()),
            provider: Some("github".to_owned()),
            trigger: Some("on-push".to_owned()),
            trigger_name: Some("on-push".to_owned()),
            run: Some(id.to_owned()),
            ..ActivityEntry::default()
        }
    }

    fn step(id: &str, status: &str, host: Option<&str>) -> RunStepView {
        RunStepView {
            id: id.to_owned(),
            step: id.to_owned(),
            position: 1,
            status: status.to_owned(),
            host: host.map(str::to_owned),
            ..RunStepView::default()
        }
    }

    fn run(status: &str, steps: Vec<RunStepView>) -> RunView {
        RunView {
            id: "run-1".to_owned(),
            project: "proj_1".to_owned(),
            trigger: "on-push".to_owned(),
            trigger_name: "on-push".to_owned(),
            status: status.to_owned(),
            steps,
            ..RunView::default()
        }
    }

    fn snapshot(detail: Option<RunDetail>, local_runs: &[&str]) -> RunsSnapshot {
        RunsSnapshot {
            filters: ActivityFilters::default(),
            entries: vec![entry("run-1", "proj_1"), entry("run-2", "proj_2")],
            local_runs: local_runs.iter().map(|run| (*run).to_owned()).collect(),
            detail,
            dispatch: None,
            notice: None,
            local_host: Some("host-local".to_owned()),
        }
    }

    #[test]
    fn a_local_run_is_marked_and_another_is_not() {
        let local: BTreeSet<String> = ["run-1".to_owned()].into_iter().collect();
        let row = ActivityRow::for_entry(&entry("run-1", "proj_1"), &local);
        assert!(row.local);
        assert_eq!(row.place_label(), Some("This machine"));
        assert!(row.line().contains("This machine"));

        let row = ActivityRow::for_entry(&entry("run-2", "proj_2"), &local);
        assert!(!row.local);
        assert_eq!(row.place_label(), None);
        assert!(!row.line().contains("This machine"));
    }

    #[test]
    fn the_detail_names_each_step_place() {
        let detail = RunDetail {
            run: run(
                "running",
                vec![
                    RunStepView {
                        position: 2,
                        ..step("ship", "pending", Some("host-other"))
                    },
                    RunStepView {
                        position: 1,
                        ..step("build", "running", Some("host-local"))
                    },
                ],
            ),
            following: true,
            problem: None,
        };
        let rows = detail_rows(&detail, Some("host-local"));
        assert_eq!(
            rows.iter().map(|row| row.step.as_str()).collect::<Vec<_>>(),
            vec!["build", "ship"],
            "steps are shown in position order, not arrival order"
        );
        assert!(rows[0].local);
        assert_eq!(rows[0].place, "This machine");
        assert!(!rows[1].local);
        assert_eq!(rows[1].place, "Another machine (host-other)");
        let heading = detail_heading(&detail, Some("host-local"));
        assert!(heading.contains("This machine"));
        assert!(heading.contains("following"));
    }

    #[test]
    fn the_detail_renders_a_named_failure_as_words_with_the_code() {
        let detail = RunDetail {
            run: run(
                "failed",
                vec![RunStepView {
                    error: Some("daemon_not_connected: host 'studio-mac' is offline".to_owned()),
                    reason: Some("daemon_not_connected".to_owned()),
                    ..step("build", "failed", Some("host-other"))
                }],
            ),
            following: false,
            problem: None,
        };
        let rows = detail_rows(&detail, Some("host-local"));
        let failure = rows[0].failure.as_deref().expect("a failure line");
        assert!(failure.contains("offline"));
        assert!(failure.contains("daemon_not_connected"));
        assert!(rows[0].line().contains("offline"));
        assert!(!detail_heading(&detail, Some("host-local")).contains("following"));

        let leading = detail_failure(&detail).expect("failure");
        assert_eq!(leading.code.as_deref(), Some("daemon_not_connected"));
    }

    #[test]
    fn the_dispatch_form_shows_every_field_in_tab_order() {
        let mut form = DispatchForm::new("proj_1", "on-push", "alice", "key-1");
        form.set_input("main");
        let lines = dispatch_lines(&form);
        assert_eq!(
            lines
                .iter()
                .map(|(label, _)| label.as_str())
                .collect::<Vec<_>>(),
            vec![
                "project",
                "trigger",
                "actor",
                "delivery key",
                "input",
                "revision"
            ]
        );
        assert_eq!(lines[4].1, "main");
    }

    #[test]
    fn the_filter_line_names_every_filter() {
        let mut filters = ActivityFilters::default();
        filters.set_project("proj_1");
        filters.set_status("failed");
        let line = filter_line(&filters);
        assert!(line.contains("proj_1"));
        assert!(line.contains("Failed"));
        assert!(line.contains("provider all"));
    }

    #[gpui::test]
    fn the_view_shows_the_local_highlight_and_the_failure_sentence(cx: &mut gpui::TestAppContext) {
        let detail = RunDetail {
            run: run(
                "failed",
                vec![RunStepView {
                    error: Some("daemon_not_connected: host 'studio-mac' is offline".to_owned()),
                    ..step("build", "failed", Some("host-local"))
                }],
            ),
            following: false,
            problem: None,
        };
        let entity =
            cx.update(|cx| cx.new(|cx| RunsView::new(snapshot(Some(detail), &["run-1"]), cx)));
        entity.read_with(cx, |view, _| {
            let rows = view.rows();
            assert_eq!(rows.len(), 2);
            assert!(rows[0].local);
            assert!(!rows[1].local);
            let steps = view.detail_rows();
            assert_eq!(steps.len(), 1);
            assert!(steps[0].local);
            assert!(view.failure_line().is_some_and(
                |line| line.contains("offline") && line.contains("daemon_not_connected")
            ));
            assert!(
                view.heading()
                    .is_some_and(|heading| heading.contains("This machine"))
            );
        });
    }

    #[gpui::test]
    fn the_view_keeps_a_foreign_project_out_of_the_rows(cx: &mut gpui::TestAppContext) {
        let mut snapshot = snapshot(None, &[]);
        snapshot.filters.set_project("proj_1");
        // The controller has already filtered; the view draws exactly what it is
        // handed, and the snapshot carries only the visible rows by construction.
        snapshot
            .entries
            .retain(|entry| snapshot.filters.matches(entry));
        let entity = cx.update(|cx| cx.new(|cx| RunsView::new(snapshot, cx)));
        entity.read_with(cx, |view, _| {
            let rows = view.rows();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].project, "proj_1");
            assert!(view.filter_line().contains("proj_1"));
        });
    }
}
