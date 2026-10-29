//! The runs tab's drawing: the activity feed, the trigger list, the live run
//! detail and the manual dispatch form.
//!
//! The renderer takes the tab's state and never changes it; the tests draw into
//! a `TestBackend` and assert the words an operator sees, including that a
//! failed step is explained in a sentence with its stable code beside it.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Cell, Paragraph, Row, Table, TableState, Wrap};

use crate::tabs::runs::{
    DispatchForm, RunsState, TAIL_LINES, entry_title, failure_explanation, kind_word, output_tail,
    run_failure_explanation, status_word, step_output,
};
use crate::ui::banner::{self, StyleRole};
use tidepool_client::types::RunView;

/// Draw the runs tab into `area`.
pub fn render(state: &RunsState, frame: &mut Frame, area: Rect) {
    let footer = footer_lines(state);
    let footer_height = footer.len().max(1) as u16;
    let [header_area, body_area, footer_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(footer_height),
    ])
    .areas(area);

    frame.render_widget(Paragraph::new(header_line(state)), header_area);
    if let Some(form) = state.form() {
        render_form(form, frame, body_area);
    } else if state.detail().is_some() || state.selected_run_id().is_some() {
        render_detail(state, frame, body_area);
    } else {
        render_feed_and_triggers(state, frame, body_area);
    }
    frame.render_widget(
        Paragraph::new(footer).wrap(Wrap { trim: false }),
        footer_area,
    );
}

fn header_line(state: &RunsState) -> String {
    if let Some(run_id) = state.selected_run_id() {
        let status = state
            .detail()
            .map(|detail| status_word(&detail.run.status))
            .unwrap_or_else(|| "Loading".to_owned());
        let follow = if state.following() {
            "following"
        } else {
            "not following"
        };
        return format!("run {run_id}  {status}  {follow}");
    }
    let visible = state.visible_entries().len();
    format!(
        "filters {}  {visible}/{} records  {} trigger(s)  {} project(s)",
        state.filters().summary(),
        state.entries().len(),
        state.triggers().len(),
        state.projects().len()
    )
}

fn footer_lines(state: &RunsState) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(form) = state.form() {
        lines.push(Line::from(format!(
            "editing {} · Tab field · Enter dispatch · Esc cancel",
            form.focus_label()
        )));
    } else if state.selected_run_id().is_some() {
        lines.push(Line::from(
            "Esc close · r read again · output follows while a step is pending or running",
        ));
    } else {
        lines.push(Line::from(
            "↑↓ select · Enter open run · d dispatch · p project · v provider · \
             t trigger · f status · x clear · r refresh",
        ));
    }
    if let Some(notice) = state.notice() {
        lines.push(Line::from(Span::styled(
            notice.to_owned(),
            banner::style(StyleRole::Warning),
        )));
    }
    lines
}

fn render_feed_and_triggers(state: &RunsState, frame: &mut Frame, area: Rect) {
    if area.width >= 100 {
        let [feed_area, trigger_area] =
            Layout::horizontal([Constraint::Percentage(62), Constraint::Percentage(38)])
                .areas(area);
        render_feed(state, frame, feed_area);
        render_triggers(state, frame, trigger_area);
    } else {
        let [feed_area, trigger_area] =
            Layout::vertical([Constraint::Percentage(64), Constraint::Percentage(36)]).areas(area);
        render_feed(state, frame, feed_area);
        render_triggers(state, frame, trigger_area);
    }
}

fn render_feed(state: &RunsState, frame: &mut Frame, area: Rect) {
    let visible = state.visible_entries();
    let block = Block::bordered().title(format!(
        " Activity ({}/{}) ",
        visible.len(),
        state.entries().len()
    ));
    if visible.is_empty() {
        let text = if state.entries().is_empty() {
            "No runs loaded yet. Press r to read the pool again."
        } else {
            "No record matches this filter."
        };
        frame.render_widget(
            Paragraph::new(text.to_owned())
                .block(block)
                .wrap(Wrap { trim: true }),
            area,
        );
        return;
    }

    let header = Row::new(
        ["KIND", "STATUS", "RECORD", "PROJECT", "AT"]
            .into_iter()
            .map(Cell::from)
            .collect::<Vec<_>>(),
    )
    .style(Style::new().add_modifier(Modifier::BOLD));

    let body: Vec<Row<'static>> = visible
        .iter()
        .map(|entry| {
            Row::new(vec![
                Cell::from(kind_word(&entry.kind)),
                Cell::from(Span::styled(
                    status_word(&entry.status),
                    status_style(&entry.status),
                )),
                Cell::from(entry_title(entry)),
                Cell::from(
                    entry
                        .project
                        .clone()
                        .or_else(|| entry.provider.clone())
                        .unwrap_or_else(|| "-".to_owned()),
                ),
                Cell::from(entry.at.clone()),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(9),
        Constraint::Length(15),
        Constraint::Min(12),
        Constraint::Length(10),
        Constraint::Length(20),
    ];
    let table = Table::new(body, widths)
        .header(header)
        .block(block)
        .column_spacing(1)
        .row_highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("▸ ");
    let mut table_state = TableState::default();
    table_state.select(Some(state.selected()));
    frame.render_stateful_widget(table, area, &mut table_state);
}

fn render_triggers(state: &RunsState, frame: &mut Frame, area: Rect) {
    let block = Block::bordered().title(format!(" Triggers ({}) ", state.triggers().len()));
    let mut lines = Vec::new();
    if state.triggers().is_empty() {
        lines.push(Line::from(
            "Choose a project with p to list its authored triggers.",
        ));
    }
    for trigger in state.triggers() {
        let state_word = if trigger.active { "active" } else { "inactive" };
        lines.push(Line::from(vec![
            Span::styled(
                trigger.name.clone(),
                Style::new().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                state_word.to_owned(),
                if trigger.active {
                    banner::style(StyleRole::Success)
                } else {
                    banner::style(StyleRole::Muted)
                },
            ),
        ]));
        lines.push(Line::from(format!("  event {}", trigger.event_line())));
        lines.push(Line::from(format!("  filters {}", trigger.filters_line())));
        if let Some(problem) = &trigger.problem {
            lines.push(Line::from(Span::styled(
                format!("  unreadable: {problem}"),
                banner::style(StyleRole::Danger),
            )));
        }
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_detail(state: &RunsState, frame: &mut Frame, area: Rect) {
    let Some(detail) = state.detail() else {
        let text = match state.detail_problem() {
            Some(problem) => format!("Could not read the run: {problem}"),
            None => "Reading the run…".to_owned(),
        };
        frame.render_widget(
            Paragraph::new(text)
                .block(Block::bordered().title(" Run "))
                .wrap(Wrap { trim: true }),
            area,
        );
        return;
    };
    let block = Block::bordered().title(format!(
        " Run {} · {} ",
        detail.run.id,
        status_word(&detail.run.status)
    ));
    frame.render_widget(
        Paragraph::new(detail_lines(state, &detail.run))
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn detail_lines(state: &RunsState, run: &RunView) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(format!(
            "trigger: {} ({})  origin: {}",
            run.trigger_name, run.trigger, run.origin
        )),
        Line::from(format!("project: {}", run.project)),
        Line::from(format!(
            "actor: {}  delivery key: {}",
            run.actor.as_deref().unwrap_or("not recorded"),
            run.delivery_key
        )),
        Line::from(format!("request: {}", run.request_text)),
        Line::from(format!(
            "created: {}  started: {}  completed: {}",
            run.created_at,
            run.started_at.as_deref().unwrap_or("-"),
            run.completed_at.as_deref().unwrap_or("-")
        )),
        Line::from(""),
        Line::from(Span::styled(
            "steps",
            banner::style(StyleRole::Focus).add_modifier(Modifier::BOLD),
        )),
    ];
    if run.steps.is_empty() {
        lines.push(Line::from("No steps recorded yet."));
    }
    for step in &run.steps {
        lines.push(Line::from(vec![
            Span::raw(format!("• {}  ", step.step)),
            Span::styled(step.status.clone(), status_style(&step.status)),
            Span::raw(format!("  {}", status_word(&step.status))),
        ]));
        let tail = output_tail(Some(step_output(step)), TAIL_LINES);
        if tail.is_empty() {
            lines.push(Line::from("    no output yet"));
        } else {
            for line in tail {
                lines.push(Line::from(format!("    {line}")));
            }
        }
        if (step.status == "failed" || step.status == "interrupted")
            && let Some(failure) =
                failure_explanation(step.error.as_deref(), step.reason.as_deref())
        {
            lines.push(Line::from(Span::styled(
                format!("    {}", failure.sentence),
                banner::style(StyleRole::Danger),
            )));
            if let Some(detail) = failure.detail {
                lines.push(Line::from(format!("    {detail}")));
            }
            lines.push(Line::from(format!(
                "    code: {}",
                failure.code.as_deref().unwrap_or("not recorded")
            )));
        }
    }
    if let Some(failure) = run_failure_explanation(run)
        && !run
            .steps
            .iter()
            .any(|step| step.status == "failed" || step.status == "interrupted")
    {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            failure.sentence,
            banner::style(StyleRole::Danger),
        )));
        lines.push(Line::from(format!(
            "code: {}",
            failure.code.as_deref().unwrap_or("not recorded")
        )));
    }
    if let Some(problem) = state.detail_problem() {
        lines.push(Line::from(Span::styled(
            format!("last read failed: {problem}"),
            banner::style(StyleRole::Warning),
        )));
    }
    lines.push(Line::from(if state.following() {
        "following: the detail is read again until the run finishes."
    } else {
        "finished: following stopped."
    }));
    lines
}

fn render_form(form: &DispatchForm, frame: &mut Frame, area: Rect) {
    let mut lines = Vec::new();
    for (index, (label, value)) in DispatchForm::labels()
        .iter()
        .zip(form.values().iter())
        .enumerate()
    {
        let focused = index == form.focus();
        let marker = if focused { "▸ " } else { "  " };
        let style = if focused {
            banner::style(StyleRole::Focus).add_modifier(Modifier::UNDERLINED)
        } else {
            Style::new()
        };
        lines.push(Line::from(vec![
            Span::raw(marker),
            Span::styled(format!("{label}: "), banner::style(StyleRole::Focus)),
            Span::styled((*value).to_owned(), style),
        ]));
    }
    if !form.issues().is_empty() {
        lines.push(Line::from(""));
        for issue in form.issues() {
            lines.push(Line::from(Span::styled(
                issue.clone(),
                banner::style(StyleRole::Danger),
            )));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(
        "The input keeps its leading key=value tokens; the rest is the request text.",
    ));
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::bordered().title(" Dispatch a run "))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn status_style(status: &str) -> Style {
    match status {
        "succeeded" | "matched" => banner::style(StyleRole::Success),
        "running" => banner::style(StyleRole::Focus),
        "pending" | "queued" | "received" => banner::style(StyleRole::Warning),
        "failed" | "rejected" => banner::style(StyleRole::Danger),
        "interrupted" => banner::style(StyleRole::Second),
        "skipped" | "unmatched" => banner::style(StyleRole::Muted),
        _ => Style::new(),
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use tidepool_client::types::RunStepView;

    use super::*;
    use crate::tabs::runs::testing::{ScriptedRuns, detail, entry, project, revision, run, step};
    use crate::tabs::runs::{RunsState, triggers_from_revision};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn draw(state: &RunsState, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| render(state, frame, frame.area()))
            .expect("draw a frame");
        let buffer = terminal.backend().buffer();
        let mut screen = String::new();
        for y in 0..height {
            for x in 0..width {
                screen.push_str(buffer[(x, y)].symbol());
            }
            screen.push('\n');
        }
        screen
    }

    fn loaded() -> RunsState {
        let now = std::time::Instant::now();
        let mut state = RunsState::new();
        state.set_member(Some("alice".to_owned()));
        let mut client = ScriptedRuns::new();
        client
            .projects
            .push_back(Ok(vec![project("proj_1", "Tidepool")]));
        client.queue_page(vec![
            entry(
                "run_1",
                "run",
                "running",
                Some("proj_1"),
                Some("github"),
                Some("on-push"),
            ),
            entry(
                "run_2",
                "step",
                "failed",
                Some("proj_1"),
                Some("github"),
                Some("on-push"),
            ),
        ]);
        client.trigger_lists.push_back(Ok(triggers_from_revision(&revision(vec![(
            "triggers/on-push.yaml",
            "name: on-push\nevent:\n  provider: github\n  type: push\nfilters:\n  connection: conn_1\n  resource:\n    kind: repository\n    id: org/repo\n  from_users:\n    - alice\nrun:\n  host: studio\n",
        )]))));
        state.drive(&mut client, now);
        state.set_project_filter_for_test("proj_1");
        state.drive(&mut client, now);
        state
    }

    #[test]
    fn the_feed_and_trigger_list_render() {
        let state = loaded();
        let screen = draw(&state, 140, 40);
        for heading in ["KIND", "STATUS", "RECORD", "PROJECT", "AT"] {
            assert!(screen.contains(heading), "{heading} missing:\n{screen}");
        }
        assert!(screen.contains("Run of 'on-push'"), "{screen}");
        assert!(screen.contains("project proj_1"), "{screen}");
        assert!(screen.contains("Triggers (1)"), "{screen}");
        assert!(screen.contains("on-push"), "{screen}");
        assert!(screen.contains("active"), "{screen}");
        assert!(screen.contains("event GitHub push"), "{screen}");
        assert!(screen.contains("repository org/repo"), "{screen}");
        assert!(screen.contains("from alice"), "{screen}");
    }

    #[test]
    fn an_empty_feed_says_how_to_load_it() {
        let state = RunsState::new();
        let screen = draw(&state, 120, 20);
        assert!(screen.contains("No runs loaded yet."), "{screen}");
        assert!(screen.contains("Choose a project"), "{screen}");
    }

    #[test]
    fn the_run_detail_carries_every_step_and_the_output_tail() {
        let now = std::time::Instant::now();
        let mut state = RunsState::new();
        let mut client = ScriptedRuns::new();
        client
            .projects
            .push_back(Ok(vec![project("proj_1", "Tidepool")]));
        client.queue_page(vec![entry(
            "run_1",
            "run",
            "running",
            Some("proj_1"),
            Some("github"),
            None,
        )]);
        client.details.push_back(Ok(detail(run(
            "run_1",
            "running",
            vec![step(
                "build",
                "running",
                Some("compiling\nlinking\nuploading the artefact"),
            )],
        ))));
        state.drive(&mut client, now);
        state.on_key(key(KeyCode::Enter));
        state.drive(&mut client, now);

        let screen = draw(&state, 140, 40);
        assert!(screen.contains("steps"), "{screen}");
        assert!(screen.contains("• build"), "{screen}");
        assert!(screen.contains("Running"), "{screen}");
        assert!(screen.contains("uploading the artefact"), "{screen}");
        assert!(screen.contains("following: the detail"), "{screen}");
        assert!(screen.contains("delivery key: key-1"), "{screen}");
    }

    #[test]
    fn a_failure_code_renders_as_words_with_the_code_available() {
        let now = std::time::Instant::now();
        let mut state = RunsState::new();
        let mut client = ScriptedRuns::new();
        client
            .projects
            .push_back(Ok(vec![project("proj_1", "Tidepool")]));
        client.queue_page(vec![entry(
            "run_1",
            "run",
            "failed",
            Some("proj_1"),
            Some("github"),
            None,
        )]);
        let failed = RunStepView {
            error: Some("daemon_not_connected: host 'studio' is offline".to_owned()),
            ..step("deploy", "failed", None)
        };
        client
            .details
            .push_back(Ok(detail(run("run_1", "failed", vec![failed]))));
        state.drive(&mut client, now);
        state.on_key(key(KeyCode::Enter));
        state.drive(&mut client, now);

        let screen = draw(&state, 160, 40);
        assert!(
            screen.contains("The host went offline before it could run this step."),
            "the sentence is shown:\n{screen}"
        );
        assert!(
            screen.contains("daemon_not_connected"),
            "the code is beside it:\n{screen}"
        );
        assert!(
            screen.contains("host 'studio' is offline"),
            "the pool detail is kept:\n{screen}"
        );
        assert!(screen.contains("finished: following stopped."), "{screen}");
    }

    #[test]
    fn the_dispatch_form_renders_its_fields_and_issues() {
        let mut state = loaded();
        state.on_key(key(KeyCode::Char('d')));
        state.on_key(key(KeyCode::Enter));
        let screen = draw(&state, 140, 40);
        assert!(screen.contains("Dispatch a run"), "{screen}");
        for label in ["project:", "trigger:", "actor:", "delivery key:", "input:"] {
            assert!(screen.contains(label), "{label} missing:\n{screen}");
        }
        assert!(screen.contains("editing project"), "{screen}");
        assert!(state.form().is_some(), "the form stayed open");
    }

    #[test]
    fn the_filter_header_names_the_combination() {
        let mut state = loaded();
        state.on_key(key(KeyCode::Char('f')));
        let screen = draw(&state, 140, 30);
        assert!(screen.contains("status Waiting to start"), "{screen}");
        assert!(screen.contains("filter"), "{screen}");
        assert!(screen.contains("0/2 records"), "{screen}");
    }

    #[test]
    fn the_tab_draws_at_a_small_and_a_large_size() {
        let state = loaded();
        for (width, height) in [(40_u16, 10_u16), (140, 40)] {
            let screen = draw(&state, width, height);
            assert!(screen.contains("Activity"), "{width}x{height}:\n{screen}");
        }
    }
}
