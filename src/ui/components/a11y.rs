//! Help for people who do not use a mouse or do not see the screen: the focus ring, names and
//! descriptions for the accessibility tree (VoiceOver reads it), tooltips that also show on
//! keyboard focus, and the small text builders that make those names.

use crate::ui::theme::Palette;
use eframe::egui::{self, accesskit, Painter, Rect, Response, Stroke, StrokeKind};

/// The place in the per-pass scratch memory where the scroll area that already scrolled to the
/// focused control leaves a note, so the area around it does not scroll to the same control.
const SCROLLED_FOR_FOCUS: &str = "scroll-follows-focus";

/// The salt of the place where each scroll area remembers the focus it saw last frame.
const FOCUS_LAST_FRAME: &str = "focus-at-last-frame";

/// The ring around the control that has the keyboard focus: 2 points, solid, in the accent
/// colour (3:1 against every background it sits on, in both appearances).
pub fn focus_ring(painter: &Painter, rect: Rect, radius: u8, p: &Palette) {
    painter.rect_stroke(rect, radius, Stroke::new(2.0, p.focus), StrokeKind::Outside);
}

/// The style of a popup menu: egui's own menu style, with the entry that has the keyboard focus
/// outlined in the accent colour (egui's menu style takes that outline away).
pub fn menu_style(p: &'static Palette) -> impl Fn(&mut egui::Style) + Send + Sync + 'static {
    move |style: &mut egui::Style| {
        egui::containers::menu::menu_style(style);
        style.visuals.widgets.active.bg_stroke = Stroke::new(1.5, p.focus);
    }
}

/// Keeps the keyboard focus in sight. Call it first in the closure of every `ScrollArea` and
/// keep the guard it returns (`let _follow = a11y::follow_focus(ui);`): when the guard is
/// dropped, at the end of the closure, it looks whether the focus has moved since the last
/// frame to a control that was drawn inside this area and, if so, scrolls the area just enough
/// to show that control with its ring. egui does not do this by itself, so without it Tab walks
/// off the bottom of the page and the ring is drawn where nobody can see it.
///
/// It only acts on a change of focus (never on a frame where the focus stays), so scrolling
/// with the wheel or the trackpad is never pulled back to the focused control. A control
/// outside the area (the tabs above it, the buttons under it) never makes it scroll: what was
/// drawn inside the closure is told by the order the controls were registered in.
pub fn follow_focus(ui: &egui::Ui) -> FocusFollower {
    let layer = ui.layer_id();
    FocusFollower {
        ctx: ui.ctx().clone(),
        layer,
        area: ui.scope_id().with(FOCUS_LAST_FRAME),
        first: ui
            .ctx()
            .viewport(|v| v.this_pass.widgets.get_layer(layer).count()),
    }
}

/// See [`follow_focus`].
#[must_use = "bind it to a variable: it scrolls when the end of the area's closure drops it"]
pub struct FocusFollower {
    ctx: egui::Context,
    layer: egui::LayerId,
    /// Where this area keeps the focus it saw at the end of the last frame.
    area: egui::Id,
    /// How many controls of the layer were registered before the area's content started.
    first: usize,
}

impl Drop for FocusFollower {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }
        let now = self.ctx.memory(|m| m.focused());
        let before = self.ctx.data(|d| d.get_temp::<Option<egui::Id>>(self.area));
        self.ctx.data_mut(|d| d.insert_temp(self.area, now));
        // The first frame of an area only learns where the focus is.
        let (Some(before), Some(id)) = (before, now) else {
            return;
        };
        if before == Some(id) {
            return;
        }
        let inside = self.ctx.viewport(|v| {
            v.this_pass
                .widgets
                .get_layer(self.layer)
                .skip(self.first)
                .any(|w| w.id == id)
        });
        let Some(mut seen) = self.ctx.read_response(id).filter(|_| inside) else {
            return;
        };
        if !seen.rect.is_positive() {
            return;
        }
        // An area inside another one scrolls first; the outer one must not then chase a
        // control that sits outside its own view.
        let pass = self.ctx.cumulative_pass_nr();
        let note = egui::Id::NULL.with(SCROLLED_FOR_FOCUS);
        if self.ctx.data(|d| d.get_temp::<(u64, egui::Id)>(note)) == Some((pass, id)) {
            return;
        }
        self.ctx.data_mut(|d| d.insert_temp(note, (pass, id)));
        // The ring is drawn a few points outside the control: show it whole.
        seen.rect = seen.rect.expand(5.0);
        seen.scroll_to_me(None);
    }
}

/// Marks a title as a heading of the given level (1 is the page title), so VoiceOver can jump
/// from heading to heading. Call it on the response of the label that draws the title.
pub fn heading(resp: &Response, level: usize) {
    resp.ctx.accesskit_node_builder(resp.id, |node| {
        // A label keeps its text in the value; a heading is named by its label.
        if let Some(text) = node.value().map(str::to_owned) {
            node.clear_value();
            node.set_label(text);
        }
        node.set_role(accesskit::Role::Heading);
        node.set_level(level);
    });
}

/// Takes a piece of text out of what a screen reader walks through, because something else
/// already says it (the button that frames it, for one).
pub fn hide(resp: &Response) {
    resp.ctx.accesskit_node_builder(resp.id, |node| {
        node.set_hidden();
    });
}

/// The name of the entry of the place the person is in: «Inicio, página actual». Said in words,
/// because VoiceOver does not read the selected flag of a button.
pub fn current_page_name(name: &str, current: bool) -> String {
    if current {
        format!("{name}, página actual")
    } else {
        name.to_string()
    }
}

/// Tells the accessibility tree what the control is: a button called `label`, `selected` when it
/// marks the place the person is in (a sidebar entry, a tab), with `description` as its longer
/// help. A selected button is not a check box: the selected flag is set on the node itself.
pub fn button_info(
    resp: &Response,
    label: &str,
    enabled: bool,
    selected: Option<bool>,
    description: Option<&str>,
) {
    node_info(
        resp,
        egui::Role::Button,
        label,
        enabled,
        selected,
        description,
    );
}

/// Like [`button_info`] for any role (a tab, for one).
pub fn node_info(
    resp: &Response,
    role: egui::Role,
    label: &str,
    enabled: bool,
    selected: Option<bool>,
    description: Option<&str>,
) {
    resp.widget_info(|| egui::WidgetInfo::labeled(role, enabled, label));
    resp.ctx.accesskit_node_builder(resp.id, |node| {
        if let Some(on) = selected {
            node.set_selected(on);
        }
        if let Some(text) = description.filter(|t| !t.is_empty()) {
            node.set_description(text);
        }
    });
}

/// Sets the accessible description of a control that already has a node.
pub fn describe(resp: &Response, text: &str) {
    if text.is_empty() {
        return;
    }
    resp.ctx.accesskit_node_builder(resp.id, |node| {
        node.set_description(text);
    });
}

/// Says that the control opens a menu.
pub fn opens_menu(resp: &Response) {
    resp.ctx.accesskit_node_builder(resp.id, |node| {
        node.set_has_popup(accesskit::HasPopup::Menu);
    });
}

/// Shows `text` as a tooltip while the control has the keyboard focus (the mouse already has
/// the hover tooltip). A click does not give a button the focus, so this only shows for Tab.
pub fn tip_on_focus(resp: &Response, text: &str) {
    if text.is_empty() || !resp.has_focus() || resp.hovered() || resp.clicked() {
        return;
    }
    // A menu opened from the same control shares the tooltip's id: never both at once.
    if egui::Popup::is_id_open(&resp.ctx, egui::Popup::default_response_id(resp)) {
        return;
    }
    resp.show_tooltip_text(text);
}

/// A name for a label that reads as text for the accessibility tree: a painted piece of text has
/// no node of its own, so it is given one here.
pub fn label_node(resp: &Response, text: &str) {
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, text));
}

/// The name of a row or card that has a second line: «Steam (copia), En uso».
pub fn name_with_state(name: &str, state: Option<&str>) -> String {
    match state.map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => format!("{name}, {s}"),
        None => name.to_string(),
    }
}

/// The text of a tooltip made of the full text of something that was cut (when it was) and a
/// longer help; either may be missing. Lines are separated by a line break.
pub fn joined_tip(full_text: Option<&str>, tip: &str) -> String {
    [full_text.unwrap_or(""), tip]
        .into_iter()
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The name of a program tile: «Steam, abierto» while it runs. The state is words, not only the
/// green dot.
pub fn tile_name(name: &str, running: bool) -> String {
    if running {
        format!("{name}, abierto")
    } else {
        name.to_string()
    }
}

/// The name of the «Más opciones» button of a tile or card: it says whose options they are.
pub fn more_options_name(name: &str) -> String {
    format!("Más opciones de {name}")
}

/// What a number tile says out loud: «2 botellas propias». A tile whose label is a name («Wine»)
/// reads «Wine: 11.0»; with nothing to show it says so.
pub fn stat_name(value: &str, label: &str) -> String {
    let value = value.trim();
    if value.is_empty() || value == "—" {
        return format!("{label}: sin datos");
    }
    if label.chars().next().is_some_and(char::is_uppercase) {
        format!("{label}: {value}")
    } else {
        format!("{value} {label}")
    }
}

/// What the keyboard focus is on in a dialog, for the decision about ↩.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogFocus {
    /// Nothing in the dialog has the focus.
    Nowhere,
    /// A text field has it: ↩ ends the typing.
    TextField,
    /// A button or another control has it: ↩ belongs to that control.
    Control,
}

/// Whether ↩ should press the dialog's main action by itself. A focused control answers ↩ on
/// its own (a focused «Cancelar» cancels), so the shortcut only applies when nothing but a text
/// field has the focus; and never for a destructive action, which has to be asked for by
/// focusing its own button.
pub fn enter_confirms(focus: DialogFocus, action_enabled: bool, destructive: bool) -> bool {
    action_enabled && !destructive && focus != DialogFocus::Control
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Enter on a focused button is that button's; the shortcut is for a text field or nothing.
    #[test]
    fn enter_belongs_to_the_focused_control() {
        use DialogFocus::{Control, Nowhere, TextField};
        // Nothing or a text field focused: the main action, when it can be taken.
        assert!(enter_confirms(Nowhere, true, false));
        assert!(enter_confirms(TextField, true, false));
        assert!(!enter_confirms(Nowhere, false, false));
        assert!(!enter_confirms(TextField, false, false));
        // A button focused (Cancelar, for one): the shortcut stays out of it.
        assert!(!enter_confirms(Control, true, false));
        assert!(!enter_confirms(Control, false, false));
        // A destructive dialog never confirms by the shortcut, whatever has the focus.
        for focus in [Nowhere, TextField, Control] {
            assert!(!enter_confirms(focus, true, true), "{focus:?}");
        }
    }

    #[test]
    fn names_say_the_state_in_words() {
        assert_eq!(
            name_with_state("Steam (copia)", Some("En uso")),
            "Steam (copia), En uso"
        );
        assert_eq!(name_with_state("Inicio", None), "Inicio");
        assert_eq!(name_with_state("Inicio", Some("  ")), "Inicio");
        assert_eq!(current_page_name("Inicio", true), "Inicio, página actual");
        assert_eq!(current_page_name("Inicio", false), "Inicio");
        assert_eq!(tile_name("Steam", true), "Steam, abierto");
        assert_eq!(tile_name("Steam", false), "Steam");
        assert_eq!(more_options_name("Steam"), "Más opciones de Steam");
    }

    #[test]
    fn tooltips_join_what_is_there() {
        assert_eq!(joined_tip(None, ""), "");
        assert_eq!(
            joined_tip(Some("Nombre largo, En uso"), ""),
            "Nombre largo, En uso"
        );
        assert_eq!(joined_tip(None, "Ayuda"), "Ayuda");
        assert_eq!(joined_tip(Some("A"), "B"), "A\nB");
    }

    #[test]
    fn number_tiles_read_as_a_sentence() {
        assert_eq!(stat_name("2", "botellas propias"), "2 botellas propias");
        assert_eq!(stat_name("0", "en uso ahora"), "0 en uso ahora");
        assert_eq!(stat_name("11.0", "Wine"), "Wine: 11.0");
        assert_eq!(stat_name("—", "Wine"), "Wine: sin datos");
        assert_eq!(stat_name("", "programas"), "programas: sin datos");
    }

    /// A page taller than its window, as the audit found: Tab used to walk off the bottom.
    mod scrolling {
        use super::*;
        use eframe::egui::{Event, Key, Modifiers, RawInput, Rect};

        fn raw(time: f64, events: Vec<Event>) -> RawInput {
            RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    egui::vec2(400.0, 200.0),
                )),
                time: Some(time),
                events,
                ..Default::default()
            }
        }

        fn tab() -> Event {
            Event::Key {
                key: Key::Tab,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }
        }

        /// A window of 200 points with 40 buttons in a scroll area. Returns the scroll offset and
        /// the top of the focused button after each frame.
        fn run(
            follow: bool,
            events_for: impl Fn(usize) -> Vec<Event>,
            frames: usize,
        ) -> Vec<(f32, f32)> {
            let ctx = egui::Context::default();
            let mut seen = vec![];
            for i in 0..frames {
                let mut offset = 0.0;
                let mut focused_top = f32::NAN;
                let mut out = ctx.run_ui(raw(i as f64 * 0.1, events_for(i)), |ui| {
                    let area = egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let _follow = follow.then(|| follow_focus(ui));
                            for n in 0..40 {
                                let r = ui.button(format!("Botón {n}"));
                                if r.has_focus() {
                                    focused_top = r.rect.top();
                                }
                            }
                        });
                    offset = area.state.clamped_offset().y;
                });
                out.textures_delta.clear();
                seen.push((offset, focused_top));
            }
            seen
        }

        /// One Tab every fifth frame, so the page has time to settle between two of them.
        fn tabs_from_frame_2(i: usize) -> Vec<Event> {
            if i >= 2 && i % 5 == 2 {
                vec![tab()]
            } else {
                vec![]
            }
        }

        /// Without the helper the focused button leaves the 200 points of the window and the page
        /// stays still; with it the page follows, one Tab at a time.
        #[test]
        fn tab_scrolls_the_focused_control_into_view() {
            let without = run(false, tabs_from_frame_2, 200);
            let (offset, top) = *without.last().unwrap();
            assert_eq!(offset, 0.0);
            assert!(top > 200.0, "the control is below the window: {top}");

            let with = run(true, tabs_from_frame_2, 200);
            let (offset, top) = *with.last().unwrap();
            assert!(offset > 100.0, "the page scrolled: {offset}");
            assert!(
                (0.0..=200.0).contains(&top),
                "the control is in view: {top}"
            );
            // And every step of the way, not only at the end: just before the next Tab the page
            // has caught up with the control the last one reached.
            for (step, (_, top)) in with.iter().enumerate().skip(6) {
                if step % 5 == 1 {
                    assert!(
                        (-1.0..=200.0).contains(top),
                        "frame {step}: the control is out of view at {top}"
                    );
                }
            }
        }

        /// A control outside the area (the tabs above it) that gets the focus does not drag the
        /// area along: only what was drawn inside the closure counts.
        #[test]
        fn only_the_controls_of_the_area_move_it() {
            let ctx = egui::Context::default();
            let header = egui::Id::NULL.with("header");
            let mut offset = 0.0;
            let mut frame = |i: usize, events: Vec<Event>| {
                let mut out = ctx.run_ui(raw(i as f64 * 0.5, events), |ui| {
                    ui.interact(
                        Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 20.0)),
                        header,
                        egui::Sense::click(),
                    );
                    let area = egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let _follow = follow_focus(ui);
                            for n in 0..40 {
                                let _ = ui.button(format!("Botón {n}"));
                            }
                        });
                    offset = area.state.clamped_offset().y;
                });
                out.textures_delta.clear();
                offset
            };
            let mut i = 0;
            let mut step = |events: Vec<Event>, frames: usize| {
                let mut at = 0.0;
                for k in 0..frames {
                    at = frame(i, if k == 0 { events.clone() } else { vec![] });
                    i += 1;
                }
                at
            };
            step(vec![], 2);
            // The header is the first stop; 25 Tabs later the area is scrolled down.
            let mut down = 0.0;
            for _ in 0..26 {
                down = step(vec![tab()], 4);
            }
            assert!(down > 100.0, "the area followed the focus down: {down}");
            // The focus goes to the header: the area stays where it is.
            ctx.memory_mut(|m| m.request_focus(header));
            assert_eq!(
                step(vec![], 6),
                down,
                "a control outside does not scroll it"
            );
        }

        /// Scrolling by hand while the focus stays put is left alone: the helper acts on a change
        /// of focus only.
        #[test]
        fn the_wheel_is_not_fought() {
            let ctx = egui::Context::default();
            let mut offsets = vec![];
            for i in 0..30 {
                let mut events = vec![];
                if i == 0 {
                    events.push(Event::PointerMoved(egui::pos2(100.0, 100.0)));
                }
                if i == 2 {
                    events.push(tab());
                }
                if i == 10 {
                    events.push(Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0.0, -300.0),
                        phase: egui::TouchPhase::Move,
                        source: egui::MouseWheelSource::Wheel,
                        modifiers: Modifiers::NONE,
                    });
                }
                let mut offset = 0.0;
                let mut out = ctx.run_ui(raw(i as f64 * 0.1, events), |ui| {
                    let area = egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let _follow = follow_focus(ui);
                            for n in 0..40 {
                                let _ = ui.button(format!("Botón {n}"));
                            }
                        });
                    offset = area.state.clamped_offset().y;
                });
                out.textures_delta.clear();
                offsets.push(offset);
            }
            // The first button has the focus and is on top; the wheel then moved the page, and
            // it stayed where the hand left it.
            assert_eq!(offsets[9], 0.0);
            assert!(offsets[29] > 100.0, "the wheel scrolled: {offsets:?}");
            assert_eq!(offsets[29], offsets[28], "nothing pulled it back");
        }
    }

    /// The accessibility tree of a frame with the components that were given names and roles.
    mod tree {
        use super::super::super::{self as c, Button, Leading, Trailing};
        use crate::ui::icons::Icon;
        use crate::ui::theme::{self, Tone};
        use eframe::egui::{self, accesskit, Event, Key, Modifiers, RawInput, Rect};

        fn ctx() -> egui::Context {
            let ctx = egui::Context::default();
            theme::install_for_tests(&ctx);
            ctx.enable_accesskit();
            ctx
        }

        fn input(time: f64, keys: &[Key]) -> RawInput {
            let mut raw = RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    egui::vec2(800.0, 600.0),
                )),
                time: Some(time),
                ..Default::default()
            };
            for key in keys {
                raw.events.push(Event::Key {
                    key: *key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                });
            }
            raw
        }

        /// The page the checks look at: a tile, a selected row, a button that cannot be used
        /// and a number tile.
        fn page(ui: &mut egui::Ui) {
            c::surface::program_tile_in(
                ui,
                "Steam",
                "Mi botella",
                c::surface::TileState::Open,
                None,
                |ui| {
                    c::menu_item(ui, "Abrir");
                },
            );
            c::nav_item(
                ui,
                Leading::Icon(Icon::Home),
                "Inicio",
                None,
                true,
                Trailing::Nothing,
                "",
            );
            Button::primary("Instalar programa")
                .disabled_reason(Some("Primero consigue Wine."))
                .show(ui);
            c::stat_tile(ui, Icon::Bottle, "2", "botellas propias", Tone::Accent);
        }

        /// One frame; the texture changes are dropped (nothing is drawn).
        fn frame(ctx: &egui::Context, raw: RawInput) -> egui::FullOutput {
            let mut out = ctx.run_ui(raw, page);
            out.textures_delta.clear();
            out
        }

        fn nodes(out: &egui::FullOutput) -> Vec<(accesskit::NodeId, accesskit::Node)> {
            out.platform_output
                .accesskit_update
                .as_ref()
                .expect("the tree is built when accessibility is on")
                .nodes
                .clone()
        }

        fn named<'a>(
            all: &'a [(accesskit::NodeId, accesskit::Node)],
            name: &str,
        ) -> &'a accesskit::Node {
            all.iter()
                .map(|(_, n)| n)
                .find(|n| n.label() == Some(name) || n.value() == Some(name))
                .unwrap_or_else(|| panic!("no node called {name:?}"))
        }

        /// Text fields and drop-downs get a name, so a screen reader says what they are for.
        #[test]
        fn fields_and_choices_have_names() {
            let ctx = ctx();
            let mut text = String::from("Steam");
            let mut run = |time: f64| {
                let mut out = ctx.run_ui(input(time, &[]), |ui| {
                    ui.add(egui::TextEdit::singleline(&mut text))
                        .accessible_name("Nombre de la botella");
                    egui::ComboBox::from_id_salt("choice")
                        .selected_text("Auto")
                        .show_ui(ui, |_| {})
                        .response
                        .accessible_name("Gráficos");
                });
                out.textures_delta.clear();
                out
            };
            let _ = run(0.0);
            let all = nodes(&run(0.1));
            assert!(all
                .iter()
                .any(|(_, n)| n.label() == Some("Nombre de la botella")));
            assert!(all.iter().any(|(_, n)| n.label() == Some("Gráficos")));
        }

        /// A title is a heading with its level, and a text that something else already says is
        /// out of the reading order; plain texts stay plain.
        #[test]
        fn titles_are_headings_and_repeated_texts_are_hidden() {
            let ctx = ctx();
            let run = |time: f64| {
                let mut out = ctx.run_ui(input(time, &[]), |ui| {
                    c::a11y::heading(&ui.label("Ajustes"), 1);
                    c::a11y::heading(&ui.label("GENERAL"), 2);
                    c::a11y::hide(&ui.label("Wine listo"));
                    ui.label("Un texto cualquiera");
                });
                out.textures_delta.clear();
                out
            };
            let _ = run(0.0);
            let all = nodes(&run(0.1));
            // The text runs inside the labels carry the same words; they are not what is read.
            let all: Vec<_> = all
                .into_iter()
                .filter(|(_, n)| n.role() != accesskit::Role::TextRun)
                .collect();
            let title = named(&all, "Ajustes");
            assert_eq!(title.role(), accesskit::Role::Heading);
            assert_eq!(title.level(), Some(1));
            assert_eq!(named(&all, "GENERAL").level(), Some(2));
            assert!(named(&all, "Wine listo").is_hidden());
            let plain = named(&all, "Un texto cualquiera");
            assert_eq!(plain.role(), accesskit::Role::Label);
            assert!(!plain.is_hidden());
        }

        #[test]
        fn the_page_has_names_roles_and_states() {
            let ctx = ctx();
            let _ = frame(&ctx, input(0.0, &[]));
            let out = frame(&ctx, input(0.1, &[]));
            let all = nodes(&out);

            let tile = named(&all, "Steam, abierto");
            assert_eq!(tile.role(), accesskit::Role::Button);

            let more = named(&all, "Más opciones de Steam");
            assert_eq!(more.role(), accesskit::Role::Button);
            assert_eq!(more.has_popup(), Some(accesskit::HasPopup::Menu));
            assert!(more.supports_action(accesskit::Action::Click));
            assert!(more.supports_action(accesskit::Action::Focus));

            // A sidebar entry is a button that is selected, not a check box, and its name says
            // that it is the page the person is on.
            let row = named(&all, "Inicio, página actual");
            assert_eq!(row.role(), accesskit::Role::Button);
            assert_eq!(row.is_selected(), Some(true));
            assert_eq!(row.toggled(), None);

            // A button that cannot be used says why, and Tab can reach it.
            let off = named(&all, "Instalar programa");
            assert!(off.is_disabled());
            assert_eq!(off.description(), Some("Primero consigue Wine."));
            assert!(off.supports_action(accesskit::Action::Focus));

            // A painted number tile reads as a sentence.
            let stat = named(&all, "2 botellas propias");
            assert_eq!(stat.role(), accesskit::Role::Label);
        }

        /// A switch that cannot be used right now still takes the focus and says why, as a
        /// disabled button does; a free one has no description.
        #[test]
        fn a_held_switch_is_reachable_and_says_why() {
            let ctx = ctx();
            let (mut a, mut b) = (false, true);
            let mut run = |time: f64, keys: &[Key]| {
                let mut out = ctx.run_ui(input(time, keys), |ui| {
                    c::toggle_held(
                        ui,
                        &mut a,
                        "Alta resolución",
                        Some("Hay una copia en marcha."),
                    );
                    c::toggle(ui, &mut b, "Otro ajuste");
                });
                out.textures_delta.clear();
                out
            };
            let _ = run(0.0, &[]);
            let out = run(0.1, &[Key::Tab]);
            let all = nodes(&out);
            let held = named(&all, "Alta resolución");
            assert_eq!(held.role(), accesskit::Role::Switch);
            assert!(held.is_disabled());
            assert_eq!(held.description(), Some("Hay una copia en marcha."));
            assert!(held.supports_action(accesskit::Action::Focus));
            let held_id = all
                .iter()
                .find(|(_, n)| n.label() == Some("Alta resolución"))
                .map(|(id, _)| *id)
                .unwrap();
            assert_eq!(
                out.platform_output.accesskit_update.as_ref().unwrap().focus,
                held_id,
                "the first Tab stop is the held switch"
            );
            let free = named(&all, "Otro ajuste");
            assert!(!free.is_disabled());
            assert_eq!(free.description(), None);
            // ↩ on it does nothing: it is held. On the free one it does (the check is not
            // blind).
            let _ = run(0.2, &[Key::Enter]);
            let _ = run(0.3, &[Key::Tab]);
            let _ = run(0.4, &[Key::Enter]);
            let _ = run(0.5, &[]);
            assert!(!a, "a held switch does not change");
            assert!(!b, "the free switch does");
        }

        /// Tab reaches the tile, then its «Más opciones» button, and ↩ on that opens the menu.
        #[test]
        fn tab_and_enter_open_the_options_menu() {
            let ctx = ctx();
            let _ = frame(&ctx, input(0.0, &[]));
            let _ = frame(&ctx, input(0.1, &[Key::Tab]));
            let out = frame(&ctx, input(0.2, &[Key::Tab]));
            let all = nodes(&out);
            let more_id = all
                .iter()
                .find(|(_, n)| n.label() == Some("Más opciones de Steam"))
                .map(|(id, _)| *id)
                .unwrap();
            assert_eq!(
                out.platform_output.accesskit_update.as_ref().unwrap().focus,
                more_id,
                "the second Tab stop is the options button"
            );
            assert!(!egui::Popup::is_any_open(&ctx));
            let _ = frame(&ctx, input(0.3, &[Key::Enter]));
            let _ = frame(&ctx, input(0.4, &[]));
            assert!(egui::Popup::is_any_open(&ctx), "↩ opens the menu");
        }
    }
}
