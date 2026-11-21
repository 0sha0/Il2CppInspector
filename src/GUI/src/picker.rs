//! Game plugin picker: one column per `GameId` segment, each sliding open once the previous column has a choice.

use std::path::PathBuf;
use std::time::Duration;

use gpui::{prelude::*, *};

use crate::{cli, ui};

/// Header and width of each column. The widths fill the launcher table; only the last column has no separator.
const COLUMNS: [(&str, f32); 4] = [("Game", 172.), ("Region", 112.), ("Platform", 124.), ("Version", 197.)];
const OPEN: Duration = Duration::from_millis(140);
/// Columns opened by one choice follow each other by this delay.
const STAGGER: Duration = Duration::from_millis(35);

enum Games {
    Loading,
    Loaded(Vec<cli::GameInfo>),
    Failed(String),
}

pub struct GamePicker {
    games: Games,
    /// Chosen segments; empty lets the CLI match a plugin from the metadata.
    path: Vec<String>,
    /// Identifies the choices each column shows; a new value replays the column's opening animation.
    revisions: [u64; COLUMNS.len()],
    next_revision: u64,
    /// First column opened by the latest choice.
    opened_from: usize,
    scrolls: [ScrollHandle; COLUMNS.len()],
}

impl GamePicker {
    pub fn new(cli: Option<PathBuf>, cx: &mut Context<Self>) -> Self {
        let games = match cli {
            None => Games::Failed(cli::NOT_FOUND.to_owned()),
            Some(cli) => {
                let games = cx.background_executor().spawn(async move { cli::list_games(&cli) });
                cx.spawn(async move |this, cx| {
                    let games = games.await;
                    let _ = this.update(cx, |this, cx| {
                        this.games = match games {
                            Ok(games) => Games::Loaded(games),
                            Err(error) => Games::Failed(error),
                        };
                        cx.notify();
                    });
                })
                .detach();
                Games::Loading
            }
        };
        Self {
            games,
            path: Vec::new(),
            revisions: [0; COLUMNS.len()],
            next_revision: 0,
            opened_from: 1,
            scrolls: std::array::from_fn(|_| ScrollHandle::new()),
        }
    }

    /// The `--game` value; `None` lets the CLI detect the plugin.
    pub fn game(&self) -> Result<Option<String>, String> {
        match self.path.len() {
            0 => Ok(None),
            depth if depth == COLUMNS.len() => Ok(Some(self.path.join("_"))),
            depth => Err(format!("Choose a {} for {}.", COLUMNS[depth].0.to_lowercase(), self.path.join(" "))),
        }
    }

    /// No declaration is selected while auto-detecting or choosing an incomplete GameId.
    pub fn inputs(&self) -> Option<cli::InputRequirements> {
        let Games::Loaded(games) = &self.games else {
            return None;
        };
        let selected = self.game().ok().flatten()?;
        games.iter().find(|game| game.game_id == selected).map(|game| game.inputs)
    }

    /// `None` is Auto-detect in the first column. Choosing an option already on the path keeps the columns after it.
    fn choose(&mut self, depth: usize, value: Option<String>, cx: &mut Context<Self>) {
        let unchanged = match &value {
            Some(value) => self.path.get(depth) == Some(value),
            None => self.path.is_empty(),
        };
        let Games::Loaded(games) = &self.games else {
            return;
        };
        if unchanged {
            return;
        }
        let mut path: Vec<String> = self.path.iter().take(depth).cloned().collect();
        path.extend(value);
        self.path = extend(games, path);
        for revision in &mut self.revisions[depth + 1..] {
            self.next_revision += 1;
            *revision = self.next_revision;
        }
        for scroll in &self.scrolls[depth + 1..] {
            scroll.set_offset(Point::default());
        }
        self.opened_from = depth + 1;
        cx.notify();
    }

    fn row(&self, depth: usize, index: usize, value: Option<String>, cx: &mut Context<Self>) -> Stateful<Div> {
        let selected = match &value {
            Some(value) => self.path.get(depth) == Some(value),
            None => self.path.is_empty(),
        };
        // The deepest choice is highlighted; earlier choices on the path are tinted.
        let current = selected && depth == self.path.len().saturating_sub(1);
        let opens = value.is_some() && depth + 1 < COLUMNS.len();
        let label = value.clone().unwrap_or_else(|| "Auto-detect".to_owned());
        div()
            .id(("option", index))
            .flex()
            .items_center()
            .gap(px(6.))
            .px(px(10.))
            .py(px(6.))
            .map(|row| {
                if current {
                    row.bg(rgba(ui::ACCENT)).text_color(rgba(ui::WHITE))
                } else if selected {
                    row.bg(rgba(ui::ACCENT_SOFT))
                } else {
                    row.when(index % 2 == 1, |row| row.bg(rgba(ui::STRIPE)))
                        .hover(|style| style.bg(rgba(ui::FILL)))
                }
            })
            .on_click(cx.listener(move |this, _, _, cx| this.choose(depth, value.clone(), cx)))
            .child(div().flex_1().min_w_0().truncate().child(label))
            .when(opens, |row| {
                row.child(
                    div()
                        .flex_shrink_0()
                        .text_color(rgba(if current { ui::ON_ACCENT_SECONDARY } else { ui::TERTIARY }))
                        .child("›"),
                )
            })
    }

    fn column(&self, depth: usize, rows: Vec<Stateful<Div>>) -> AnyElement {
        let (title, width) = COLUMNS[depth];
        let content = div()
            .w(px(width))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .when(depth + 1 < COLUMNS.len(), |content| {
                content.border_r_1().border_color(rgba(ui::SEPARATOR))
            })
            .child(header(title))
            .child(ui::scroll_area(
                ("game-options", depth),
                &self.scrolls[depth],
                false,
                div().flex().flex_col().children(rows),
            ));
        // The clipping wrapper grows to the column width while the right-aligned content slides out from behind the
        // previous column.
        let column = div().flex_shrink_0().flex().justify_end().overflow_hidden().child(content);
        if depth == 0 {
            return column.into_any_element();
        }
        let delay = STAGGER * depth.saturating_sub(self.opened_from) as u32;
        column
            .with_animations(
                ("game-column", self.revisions[depth]),
                vec![
                    Animation::new(delay.max(Duration::from_millis(1))),
                    Animation::new(OPEN).with_easing(ease_out_quint()),
                ],
                move |column, phase, delta| {
                    let open = if phase == 0 { 0. } else { delta };
                    column.w(px(width * open)).opacity(open)
                },
            )
            .into_any_element()
    }
}

impl Render for GamePicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut columns: Vec<Vec<Option<String>>> = vec![vec![None]];
        let note = match &self.games {
            Games::Loading => Some(ui::message("Loading game plugins…")),
            Games::Failed(error) => Some(div().p(px(10.)).text_color(rgba(ui::DANGER_TEXT)).child(error.clone())),
            Games::Loaded(games) => {
                columns[0].extend(options(games, &[]).into_iter().map(|option| Some(option.to_owned())));
                for depth in 1..=self.path.len().min(COLUMNS.len() - 1) {
                    columns.push(
                        options(games, &self.path[..depth])
                            .into_iter()
                            .map(|option| Some(option.to_owned()))
                            .collect(),
                    );
                }
                None
            }
        };
        let mut elements = Vec::with_capacity(columns.len());
        for (depth, values) in columns.into_iter().enumerate() {
            let mut rows = Vec::with_capacity(values.len());
            for (index, value) in values.into_iter().enumerate() {
                rows.push(self.row(depth, index, value, cx));
            }
            elements.push(self.column(depth, rows));
        }
        // Only the table gives up height in the launcher; each column then scrolls on its own.
        div()
            .flex()
            .min_h(px(150.))
            .rounded(px(8.))
            .border_1()
            .border_color(rgba(ui::SEPARATOR))
            .bg(rgba(ui::TABLE))
            .overflow_hidden()
            .children(elements)
            .child(div().flex_1().min_w_0().flex().flex_col().child(header("")).children(note))
    }
}

fn header(title: &'static str) -> Div {
    div()
        .h(px(26.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .px(px(10.))
        .bg(rgba(ui::TABLE_HEADER))
        .border_b_1()
        .border_color(rgba(ui::SEPARATOR))
        .text_color(rgba(ui::TERTIARY))
        .text_size(px(11.))
        .font_weight(FontWeight::MEDIUM)
        .child(title)
}

/// `GamePlugins.Get` accepts only `NAME_REGION_PLATFORM_VERSION` with four non-empty parts.
fn segments(game: &str) -> Option<[&str; COLUMNS.len()]> {
    let mut parts = game.split('_');
    let segments = [parts.next()?, parts.next()?, parts.next()?, parts.next()?];
    (parts.next().is_none() && segments.iter().all(|segment| !segment.is_empty())).then_some(segments)
}

/// Distinct segments after `prefix` among the GameIds starting with it, in list order.
fn options<'a>(games: &'a [cli::GameInfo], prefix: &[String]) -> Vec<&'a str> {
    let depth = prefix.len();
    let mut options = Vec::new();
    for segments in games.iter().filter_map(|game| segments(&game.game_id)) {
        if segments.iter().zip(prefix).all(|(segment, chosen)| *segment == chosen.as_str()) && !options.contains(&segments[depth]) {
            options.push(segments[depth]);
        }
    }
    options
}

/// Takes every following level that offers one choice, so a game with a single build opens all its columns at once.
fn extend(games: &[cli::GameInfo], mut path: Vec<String>) -> Vec<String> {
    while !path.is_empty() && path.len() < COLUMNS.len() {
        match options(games, &path).as_slice() {
            [only] => path.push((*only).to_owned()),
            _ => break,
        }
    }
    path
}

#[cfg(test)]
mod tests {
    // Not a glob import: gpui exports its own `test` attribute, which would shadow the built-in one.
    use super::{extend, options, segments};

    fn games() -> Vec<crate::cli::GameInfo> {
        [
            "Arknights_CN_Windows_2.7.91",
            "Genshin_CN_Windows_7.1.0",
            "Genshin_CN_Windows_7.2.0",
            "HSR_CN_Windows_4.6.51",
            "HSR_OS_Windows_4.6.0",
            "Stock",
        ]
        .map(|game_id| crate::cli::GameInfo {
            game_id: game_id.to_owned(),
            inputs: crate::cli::InputRequirements {
                binary: true,
                metadata: true,
                startup_metadata: false,
            },
        })
        .to_vec()
    }

    fn path(segments: &[&str]) -> Vec<String> {
        segments.iter().map(|segment| (*segment).to_owned()).collect()
    }

    #[test]
    fn accepts_only_four_part_game_ids() {
        assert_eq!(
            segments("NSH_OS_Windows_20260731072817UTC"),
            Some(["NSH", "OS", "Windows", "20260731072817UTC"])
        );
        assert_eq!(segments("Stock"), None);
        assert_eq!(segments("A__Windows_1.0"), None);
        assert_eq!(segments("A_CN_Windows_1.0_extra"), None);
    }

    #[test]
    fn lists_distinct_choices_under_a_prefix() {
        assert_eq!(options(&games(), &[]), ["Arknights", "Genshin", "HSR"]);
        assert_eq!(options(&games(), &path(&["HSR"])), ["CN", "OS"]);
        assert_eq!(options(&games(), &path(&["Genshin", "CN", "Windows"])), ["7.1.0", "7.2.0"]);
    }

    #[test]
    fn takes_single_choices_until_a_column_offers_several() {
        assert_eq!(extend(&games(), path(&["Arknights"])), path(&["Arknights", "CN", "Windows", "2.7.91"]));
        assert_eq!(extend(&games(), path(&["Genshin"])), path(&["Genshin", "CN", "Windows"]));
        assert_eq!(extend(&games(), path(&["HSR"])), path(&["HSR"]));
        assert_eq!(extend(&games(), path(&["HSR", "OS"])), path(&["HSR", "OS", "Windows", "4.6.0"]));
        assert_eq!(extend(&games(), Vec::new()), Vec::<String>::new());
    }
}
