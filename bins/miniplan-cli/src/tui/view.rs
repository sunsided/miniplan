use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};

use crate::tui::app::{App, AppFocus, InputField, ModalKind, SolvingState};

pub fn render(frame: &mut Frame, app: &mut App) {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(frame.area());

    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(main_chunks[0]);

    let mid_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(main_chunks[1]);

    let solver_inner = Rect {
        x: mid_chunks[0].x + 1,
        y: mid_chunks[0].y + 1,
        width: mid_chunks[0].width.saturating_sub(2),
        height: mid_chunks[0].height.saturating_sub(2),
    };

    let config_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(solver_inner);

    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(1)])
        .split(mid_chunks[1]);

    app.domain_area_rect = Some(top_chunks[0]);
    app.problem_area_rect = Some(top_chunks[1]);
    app.config_area_rect = Some(mid_chunks[0]);
    app.plan_area_rect = Some(right_chunks[1]);
    app.config_planner_row = config_chunks[0].y;
    app.config_heuristic_row = config_chunks[1].y;

    render_domain_panel(frame, app, top_chunks[0]);
    render_problem_panel(frame, app, top_chunks[1]);
    render_solver_config(frame, app, mid_chunks[0], &config_chunks);
    render_stats_panel(frame, app, right_chunks[0]);
    render_plan_panel(frame, app, right_chunks[1]);
    render_command_bar(frame, app, main_chunks[2]);

    if let Some(ref msg) = app.flash_error {
        let flash = Paragraph::new(msg.clone()).style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
        let flash_area = Rect {
            x: frame.area().width / 2 - 20,
            y: frame.area().height / 2,
            width: 40,
            height: 1,
        };
        frame.render_widget(flash, flash_area);
    }

    if app.modal.is_some() {
        render_modal(frame, app);
    } else {
        app.modal_area_rect = None;
    }
}

fn render_domain_panel(frame: &mut Frame, app: &mut App, area: Rect) {
    let is_focused = matches!(app.focus, AppFocus::DomainList);
    let block = Block::default()
        .title(" Domain ")
        .borders(Borders::ALL)
        .border_style(if is_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        });
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let items: Vec<ListItem> = app
        .domain_state
        .items
        .iter()
        .map(|&bundle_idx| {
            let (_, domain) = &app.bundle.domains[bundle_idx];
            let name = domain.name().to_string();
            let count = app.problem_count_for_domain(&name);
            let display_name = if app.duplicated_domains.contains(&name) {
                format!("{}*", name)
            } else {
                name
            };
            let label = if count > 0 {
                format!(
                    "{} ({} problem{})",
                    display_name,
                    count,
                    if count == 1 { "" } else { "s" }
                )
            } else {
                format!("{} (0 problems)", display_name)
            };
            let style = if count > 0 {
                Style::default().fg(Color::White)
            } else {
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::DIM)
            };
            ListItem::new(Span::styled(label, style))
        })
        .collect();

    let list = List::new(items).highlight_style(if is_focused {
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().bg(Color::Rgb(50, 50, 50))
    });
    frame.render_stateful_widget(list, inner, &mut app.domain_state.state);
}

fn render_problem_panel(frame: &mut Frame, app: &mut App, area: Rect) {
    let is_focused = matches!(app.focus, AppFocus::ProblemList);
    let blocked = app.problem_blocked;
    let title = if blocked {
        " Problem (blocked) "
    } else {
        " Problem "
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(if is_focused {
            Style::default().fg(Color::Cyan)
        } else if blocked {
            Style::default().fg(Color::Red)
        } else {
            Style::default()
        });
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if blocked {
        let msg = Paragraph::new("No problems match the selected domain.").style(
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::DIM),
        );
        frame.render_widget(msg, inner);
        return;
    }

    let items: Vec<ListItem> = app
        .problem_state
        .items
        .iter()
        .map(|&prob_idx| {
            let (_, problem) = &app.bundle.problems[prob_idx];
            ListItem::new(Span::styled(
                problem.name().to_string(),
                Style::default().fg(Color::White),
            ))
        })
        .collect();

    let list = List::new(items).highlight_style(if is_focused {
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().bg(Color::Rgb(50, 50, 50))
    });
    frame.render_stateful_widget(list, inner, &mut app.problem_state.state);
}

fn render_solver_config(frame: &mut Frame, app: &mut App, outer: Rect, chunks: &[Rect]) {
    let is_config_focused = matches!(app.focus, AppFocus::InputField(_) | AppFocus::SolveButton);
    let block = Block::default()
        .title(" Solver Settings ")
        .borders(Borders::ALL)
        .border_style(if is_config_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        });
    frame.render_widget(block, outer);

    let planner = &app.planners[app.selected_planner];
    let heuristic = &app.heuristics[app.selected_heuristic];

    let planner_label = format!(
        "Planner: {}  ({})  [click/Enter]",
        planner.name, planner.description
    );
    let planner_fg = if matches!(app.focus, AppFocus::InputField(_)) {
        Color::Cyan
    } else {
        Color::White
    };
    let planner_para = Paragraph::new(Span::styled(planner_label, Style::default().fg(planner_fg)));
    frame.render_widget(planner_para, chunks[0]);

    if app.heuristic_enabled {
        let heuristic_label = format!(
            "Heuristic: {}  ({})  [click/Enter]",
            heuristic.name, heuristic.description
        );
        let heuristic_fg = if matches!(app.focus, AppFocus::InputField(_)) {
            Color::Cyan
        } else {
            Color::White
        };
        let heuristic_para = Paragraph::new(Span::styled(
            heuristic_label,
            Style::default().fg(heuristic_fg),
        ));
        frame.render_widget(heuristic_para, chunks[1]);
    }

    let is_timeout_editing = app.editing_input == Some(InputField::Timeout);
    let is_maxnodes_editing = app.editing_input == Some(InputField::MaxNodes);
    let is_outputpath_editing = app.editing_input == Some(InputField::OutputPath);
    let is_timeout_focused = matches!(app.focus, AppFocus::InputField(InputField::Timeout));
    let is_maxnodes_focused = matches!(app.focus, AppFocus::InputField(InputField::MaxNodes));
    let is_outputpath_focused = matches!(app.focus, AppFocus::InputField(InputField::OutputPath));

    let timeout_hint = if is_timeout_editing {
        " (editing)"
    } else if is_timeout_focused {
        " [Enter]"
    } else {
        ""
    };
    let maxnodes_hint = if is_maxnodes_editing {
        " (editing)"
    } else if is_maxnodes_focused {
        " [Enter]"
    } else {
        ""
    };
    let outputpath_hint = if is_outputpath_editing {
        " (editing)"
    } else if is_outputpath_focused {
        " [Enter]"
    } else {
        ""
    };

    let lines = vec![
        Line::from(vec![
            Span::styled(
                format!("Timeout:{} ", timeout_hint),
                Style::default()
                    .fg(if is_timeout_focused {
                        Color::Cyan
                    } else {
                        Color::White
                    })
                    .add_modifier(if is_timeout_focused {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
            Span::styled(
                if is_timeout_editing {
                    format!("{}█", app.timeout_input)
                } else {
                    app.timeout_input.clone()
                },
                Style::default().fg(if is_timeout_editing {
                    Color::Yellow
                } else {
                    Color::Gray
                }),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("Max Nodes:{} ", maxnodes_hint),
                Style::default()
                    .fg(if is_maxnodes_focused {
                        Color::Cyan
                    } else {
                        Color::White
                    })
                    .add_modifier(if is_maxnodes_focused {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
            Span::styled(
                if is_maxnodes_editing {
                    format!("{}█", app.max_nodes_input)
                } else {
                    app.max_nodes_input.clone()
                },
                Style::default().fg(if is_maxnodes_editing {
                    Color::Yellow
                } else {
                    Color::Gray
                }),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("Output:{} ", outputpath_hint),
                Style::default()
                    .fg(if is_outputpath_focused {
                        Color::Cyan
                    } else {
                        Color::White
                    })
                    .add_modifier(if is_outputpath_focused {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
            Span::styled(
                if is_outputpath_editing {
                    format!("{}█", app.output_path)
                } else {
                    app.output_path.clone()
                },
                Style::default().fg(if is_outputpath_editing {
                    Color::Yellow
                } else {
                    Color::Gray
                }),
            ),
        ]),
    ];
    let para = Paragraph::new(lines);
    frame.render_widget(para, chunks[2]);

    let can_solve = app.can_solve();
    let is_solving = app.solving_state == SolvingState::Running;
    let solve_text = if is_solving {
        "Solving...".to_string()
    } else if can_solve {
        "[ Solve ]".to_string()
    } else {
        "[ Solve (blocked) ]".to_string()
    };
    let solve_fg = if can_solve && !is_solving {
        Color::Green
    } else {
        Color::DarkGray
    };
    let solve_style = if can_solve && !is_solving {
        Style::default().fg(solve_fg).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(solve_fg).add_modifier(Modifier::DIM)
    };
    let solve_para = Paragraph::new(Span::styled(solve_text, solve_style));
    frame.render_widget(solve_para, chunks[3]);
    app.solve_button_rect = Some(chunks[3]);
}

fn render_stats_panel(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default().title(" Statistics ").borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = match &app.solving_state {
        SolvingState::Idle => {
            vec![Line::from(Span::styled(
                "Ready to solve",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::DIM),
            ))]
        }
        SolvingState::Running => {
            vec![Line::from(Span::styled(
                "Solving...",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ))]
        }
        SolvingState::Completed | SolvingState::Failed => match &app.outcome {
            Some(Ok(miniplan::search::SearchOutcome::Plan(_, stats))) => {
                vec![
                    Line::from(Span::styled(
                        "Plan found!",
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    )),
                    Line::from(format!(
                        "Cost: {:.2}  Length: {}",
                        stats.plan_cost, stats.plan_length
                    )),
                    Line::from(format!(
                        "Expanded: {}  Generated: {}",
                        stats.nodes_expanded, stats.nodes_generated
                    )),
                    Line::from(format!("Time: {:?}", stats.elapsed)),
                ]
            }
            Some(Ok(miniplan::search::SearchOutcome::Unsolvable(stats))) => {
                vec![
                    Line::from(Span::styled(
                        "Unsolvable",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    )),
                    Line::from(format!("Expanded: {}", stats.nodes_expanded)),
                    Line::from(format!("Time: {:?}", stats.elapsed)),
                ]
            }
            Some(Ok(miniplan::search::SearchOutcome::LimitReached(stats))) => {
                vec![
                    Line::from(Span::styled(
                        "Limit reached",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    )),
                    Line::from(format!("Expanded: {}", stats.nodes_expanded)),
                    Line::from(format!("Time: {:?}", stats.elapsed)),
                ]
            }
            Some(Err(e)) => {
                vec![
                    Line::from(Span::styled(
                        "Error",
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    )),
                    Line::from(e.clone()),
                ]
            }
            _ => vec![Line::from("No stats available")],
        },
    };

    let para = Paragraph::new(lines);
    frame.render_widget(para, inner);
}

fn render_plan_panel(frame: &mut Frame, app: &App, area: Rect) {
    let is_focused = matches!(app.focus, AppFocus::PlanScroll);
    let block = Block::default()
        .title(" Plan ")
        .borders(Borders::ALL)
        .border_style(if is_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        });
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines: Vec<Line> = match &app.solved_plan {
        None => {
            let msg = match &app.solving_state {
                SolvingState::Running => "Solving...",
                SolvingState::Failed => "Solving failed.",
                _ => "No plan yet — select a problem and press s",
            };
            vec![Line::from(Span::styled(
                msg,
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::DIM),
            ))]
        }
        Some(plan) => {
            let mut items = vec![Line::from(Span::styled(
                format!("{} steps", plan.steps.len()),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ))];
            items.push(Line::from(""));
            for (i, step) in plan.steps.iter().enumerate() {
                items.push(Line::from(format!("  {}. {}", i + 1, step.op_name)));
            }
            items
        }
    };

    let visible_start = app.plan_scroll_offset.min(lines.len().saturating_sub(1));
    let visible_lines: Vec<Line> = lines.iter().skip(visible_start).cloned().collect();

    let para = Paragraph::new(visible_lines);
    frame.render_widget(para, inner);

    if app.solved_plan.is_some() && lines.len() > inner.height as usize {
        let scroll_hint = Paragraph::new(Span::styled(
            format!(
                "(scroll: ↑/↓ or mouse, {}/{})",
                visible_start + 1,
                lines.len()
            ),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::DIM),
        ));
        let hint_rect = Rect {
            x: inner.x,
            y: inner.y + inner.height - 1,
            width: inner.width,
            height: 1,
        };
        frame.render_widget(scroll_hint, hint_rect);
    }
}

fn render_command_bar(frame: &mut Frame, app: &App, area: Rect) {
    let is_focused = matches!(app.focus, AppFocus::CommandBar);
    let block = Block::default()
        .title(" Commands ")
        .borders(Borders::ALL)
        .border_style(if is_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        });
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let is_solving = app.solving_state == SolvingState::Running;
    let has_plan = app.solved_plan.is_some();

    let mut commands = Vec::new();

    if is_solving {
        commands.push(Span::styled(
            "[Esc] Cancel ",
            Style::default().fg(Color::Yellow),
        ));
    } else {
        commands.push(Span::styled(
            "[s] Solve ",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ));
    }

    if has_plan {
        commands.push(Span::styled(
            "[w] Write Plan ",
            Style::default().fg(Color::Cyan),
        ));
    }

    if app.outcome.is_some() || app.solved_plan.is_some() {
        commands.push(Span::styled(
            "[c] Clear ",
            Style::default().fg(Color::White),
        ));
        commands.push(Span::styled(
            "[r] Restart ",
            Style::default().fg(Color::White),
        ));
    }

    commands.push(Span::styled("[q] Quit", Style::default().fg(Color::Red)));

    let line = Line::from(commands);
    let para = Paragraph::new(line);
    frame.render_widget(para, inner);
}

fn render_modal(frame: &mut Frame, app: &mut App) {
    let (title, items): (&str, Vec<ListItem>) = match app.modal {
        Some(ModalKind::Planner) => {
            let its: Vec<ListItem> = app
                .planners
                .iter()
                .map(|p| ListItem::new(format!("{} — {}", p.name, p.description)))
                .collect();
            ("Select Planner", its)
        }
        Some(ModalKind::Heuristic) => {
            let its: Vec<ListItem> = app
                .heuristics
                .iter()
                .map(|h| ListItem::new(format!("{} — {}", h.name, h.description)))
                .collect();
            ("Select Heuristic", its)
        }
        None => return,
    };

    let area = frame.area();
    let modal_width = 60.min(area.width.saturating_sub(4));
    let modal_height = (items.len() as u16 + 2)
        .min(area.height.saturating_sub(4))
        .max(5);
    let modal_area = Rect {
        x: (area.width - modal_width) / 2,
        y: (area.height - modal_height) / 2,
        width: modal_width,
        height: modal_height,
    };
    app.modal_area_rect = Some(modal_area);

    frame.render_widget(Clear, modal_area);

    let block = Block::default()
        .title(format!(" {} ", title))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    frame.render_widget(&block, modal_area);

    let inner = block.inner(modal_area);
    let list = List::new(items).highlight_style(
        Style::default()
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    );
    let mut state = app.modal_list_state.clone();
    frame.render_stateful_widget(list, inner, &mut state);
}
