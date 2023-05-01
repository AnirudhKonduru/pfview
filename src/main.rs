mod pennfat;

use std::{cmp::Ordering, io, process::exit, sync::mpsc, thread};

use chrono::prelude::*;
use colored::Colorize;
use crossterm::{
    event::{self, Event as CEvent, KeyCode},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use pennfat::PennFat;
use std::time::{Duration, Instant};
use tui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier},
    text::{Span, Spans},
    widgets::{Block, BorderType, Borders, List, ListState, Paragraph, Wrap},
};

use anyhow::Result;
use tui::style::Style;
use tui::Terminal;

/// Events that can be sent to the main loop
enum Event<I> {
    /// Input event (key press)
    Input(I),
    /// Tick event, for updating the screen
    Tick,
}

/// make a paragraph with the overview of the filesystem
fn make_overview(fs: &PennFat) -> Paragraph {
    let last_update_time: DateTime<Utc> = fs.last_update_time().into();
    let overview_string = format!(
        "fat size = {} ({} entries max), block size: {}, # data blocks = {}, last updated: {}",
        fs.fat_size(),
        fs.num_fat_entries(),
        fs.block_size(),
        fs.data_block_count(),
        last_update_time.format("%Y-%m-%d %H:%M:%S")
    );
    Paragraph::new(overview_string)
        .style(Style::default().fg(Color::LightCyan))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::White))
                .title(format!("PennFat Overview"))
                .border_type(BorderType::Plain),
        )
}

/// set of instructions to display in the help box
static INSTRUCTIONS: [[&str; 2]; 7] = [
    ["q", "quit"],
    ["r", "view in raw mode"],
    ["d", "view in directory mode"],
    ["t", "toggle (raw/dir)"],
    ["j/↓", "move down a block"],
    ["k/↑", "move up a block"],
    ["l/->", "move to next block in file"],
];

/// make a paragraph with the instructions
fn make_instructions() -> Paragraph<'static> {
    let spans = INSTRUCTIONS
        .iter()
        .map(|x| {
            let key = Span::styled(
                x[0],
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            );
            let desc = Span::styled(x[1], Style::default().fg(Color::White));
            let line = vec![key, Span::raw(": "), desc];
            line
        })
        .collect::<Vec<Vec<Span>>>()
        .join(&Span::raw(" | "));

    // covert the vector of spans to a paragraph
    Paragraph::new(Spans::from(spans))
        .style(Style::default().fg(Color::LightCyan))
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::White))
                .title("Help")
                .border_type(BorderType::Plain),
        )
}

/// make a list view of the FAT table
fn make_fat_table_view<'a>(fat_table: &'a Vec<(u16, u16)>) -> List<'a> {
    // display the FAT table on the left. This is a list of all the occupied blocks,
    // and the block they point to, if any. Convert to ListItem
    let list_items = fat_table
        .iter()
        .map(|(block_num, next_block)| {
            let block_num = format!("{:04x}", block_num);
            let next_block = format!("{:04x}", next_block);
            tui::widgets::ListItem::new(Spans::from(vec![
                Span::raw(block_num),
                Span::raw(" -> "),
                Span::raw(next_block),
            ]))
        })
        .collect::<Vec<_>>();

    let fat_table_block = Block::default()
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::White))
        .title("Fat Table")
        .border_type(BorderType::Plain);

    List::new(list_items)
        .block(fat_table_block)
        .highlight_style(
            Style::default()
                .bg(Color::Yellow)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
}

fn main() -> Result<()> {
    // accept one command line argument
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        println!(
            "{}",
            format!(
                "pfview {} - TUI PennFat viewer\nby {}",
                env!("CARGO_PKG_VERSION"),
                env!("CARGO_PKG_AUTHORS")
            )
            .bright_black()
        );
        // print usage in color and exit
        println!("{}", format!("Usage: {} <filename>", args[0]));
        exit(1);
    }

    let (tx, rx) = mpsc::channel();
    // how often do we want to reload the file and redraw (when there are no events)?
    // Note that decreasing this value will cause CPU usage, but probably not more than
    // 2-3% (of one core). At 700ms, it's at 0.5-0.7%% on my machine.
    let tick_rate = Duration::from_millis(700);
    thread::spawn(move || {
        let mut last_tick = Instant::now();
        loop {
            let timeout = tick_rate
                .checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| Duration::from_secs(0));

            if event::poll(timeout).expect("poll works") {
                if let CEvent::Key(key) = event::read().expect("can read events") {
                    tx.send(Event::Input(key)).expect("can send events");
                }
            }

            if last_tick.elapsed() >= tick_rate {
                if let Ok(_) = tx.send(Event::Tick) {
                    last_tick = Instant::now();
                }
            }
        }
    });

    let mut fs = PennFat::load(&args[1])?;

    enable_raw_mode().expect("can run in raw mode");
    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    // state
    let mut list_selected_state = ListState::default();
    list_selected_state.select(Some(0));
    let mut raw_mode = false;

    // loop to draw the tui
    loop {
        fs.reload()?;
        let fat_table: Vec<(u16, u16)> = fs.get_fat_table();

        terminal.draw(|rect| {
            let size = rect.size();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(
                    [
                        Constraint::Length(3),
                        Constraint::Min(2),
                        Constraint::Length(4),
                    ]
                    .as_ref(),
                )
                .split(size);

            let body_rect = chunks[1];
            rect.render_widget(make_overview(&fs), chunks[0]);
            rect.render_widget(make_instructions(), chunks[2]);

            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(15), Constraint::Min(10)].as_ref())
                .split(body_rect);

            let block_rect = chunks[1];
            rect.render_stateful_widget(
                make_fat_table_view(&fat_table),
                chunks[0],
                &mut list_selected_state,
            );

            // clear the right chuck to overwrite the previous block
            rect.render_widget(Paragraph::new("".to_owned()), block_rect);

            // display the selected block on the right
            let selected = list_selected_state.selected().unwrap_or(0);
            let block_string = if selected >= fat_table.len() {
                "nothing selected".to_owned()
            } else {
                let block_num = fat_table[selected].0;
                let block = fs.get_block(block_num);

                match (raw_mode, block) {
                    (true, Ok(block)) => block.as_raw(),
                    (_, Err(e)) => format!("error reading block: {}", e),
                    (false, Ok(block)) => {
                        let mut block_string = String::new();
                        let dentries = block.as_dentries();

                        for dentry in dentries {
                            block_string.push_str(&format!("{}\n", dentry.to_string()));
                        }
                        block_string
                    }
                }
            };

            // set block trailing space blank to avoid old text showing up

            let block = Paragraph::new(block_string)
                .style(Style::default().fg(Color::LightCyan))
                .alignment(Alignment::Left)
                .wrap(Wrap { trim: false })
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .style(Style::default().fg(Color::White))
                        .title("block")
                        .border_type(BorderType::Plain),
                );
            rect.render_widget(block, block_rect);
        })?;

        match rx.recv()? {
            Event::Input(event) => match event.code {
                KeyCode::Char('q') => {
                    disable_raw_mode()?;
                    terminal.show_cursor()?;
                    break;
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    let selected = list_selected_state.selected().unwrap_or(0);
                    if selected < fat_table.len() - 1 {
                        list_selected_state.select(Some(selected + 1));
                    }
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    let selected = list_selected_state.selected().unwrap_or(0);
                    if selected > 0 {
                        list_selected_state.select(Some(selected - 1));
                    }
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    let selected = list_selected_state.selected().unwrap_or(0);
                    if selected < fat_table.len() - 1 {
                        let next = fat_table[selected as usize].1;
                        if next != 0 && next != 0xffff {
                            // binary search through the confirm if the next block is in the fat table
                            let f = fat_table.binary_search_by(|probe| {
                                if probe.0 < next {
                                    Ordering::Less
                                } else if probe.0 > next {
                                    Ordering::Greater
                                } else {
                                    Ordering::Equal
                                }
                            });
                            if let Ok(i) = f {
                                list_selected_state.select(Some(i));
                            }
                        }
                    }
                }
                KeyCode::Char('t') => {
                    raw_mode = !raw_mode;
                }
                KeyCode::Char('r') => {
                    raw_mode = true;
                }
                KeyCode::Char('d') => {
                    raw_mode = false;
                }

                _ => {}
            },

            Event::Tick => {}
        }
    }

    Ok(())
}
