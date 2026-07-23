mod best_scores;
mod game;

use best_scores::BestScores;
use game::*;
use macroquad::prelude::*;

const WIN_SCORE: u32 = 5;

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    VsAi,
    TwoPlayer,
    Solo,
}

enum Screen {
    Menu,
    Playing(Play),
    GameOver { message: String },
}

struct Play {
    mode: Mode,
    ball: Ball,
    left: Paddle,
    right: Paddle,
    score_left: u32,
    score_right: u32, // doubles as bounce count in solo mode
    serve: Option<Side>, // Some(side) = ball resting at that paddle, waiting for it to move
    paused: bool,
}

impl Play {
    fn new(mode: Mode) -> Play {
        let left = Paddle::new(20.0);
        Play {
            mode,
            ball: serve_from(left, Side::Left, false),
            left,
            right: Paddle::new(COURT_W - 20.0 - PADDLE_W),
            score_left: 0,
            score_right: 0,
            serve: Some(Side::Left),
            paused: false,
        }
    }
}

fn key_dir(up: KeyCode, down: KeyCode) -> f32 {
    match (is_key_down(up), is_key_down(down)) {
        (true, false) => -1.0,
        (false, true) => 1.0,
        _ => 0.0,
    }
}

/// Per-frame touch steering: target court-space center y for each paddle,
/// or None if untouched. Any touch drives the left paddle, except in
/// two-player where the court splits at the middle (left/right halves in
/// landscape, bottom/top halves in portrait). Drag tracks the pointer 1:1
/// (see `set_paddle_center`) rather than chasing it at a capped speed.
fn touch_targets(p: &Play) -> (Option<f32>, Option<f32>) {
    let mut targets = (None, None);
    // a held left mouse button steers like a finger, so desktop gets drag control too
    let mouse = is_mouse_button_down(MouseButton::Left)
        .then(|| mouse_position())
        .map(|(x, y)| vec2(x, y));
    for pos in touches()
        .iter()
        .filter(|t| !matches!(t.phase, TouchPhase::Ended | TouchPhase::Cancelled))
        .map(|t| t.position)
        .chain(mouse)
    {
        let (cx, cy) = to_court(pos.x, pos.y);
        let drives_left = p.mode != Mode::TwoPlayer || cx < COURT_W / 2.0;
        if drives_left {
            targets.0 = Some(cy);
        } else {
            targets.1 = Some(cy);
        }
    }
    targets
}

/// Advance one frame; returns the next screen.
fn update_play(mut p: Play, dt: f32, best: &mut BestScores) -> Screen {
    let (touch_left, touch_right) = touch_targets(&p);

    // one human plays the left paddle alone, so accept either key layout
    let left_key_dir = match p.mode {
        Mode::VsAi | Mode::Solo => {
            (key_dir(KeyCode::W, KeyCode::S) + key_dir(KeyCode::Up, KeyCode::Down)).clamp(-1.0, 1.0)
        }
        Mode::TwoPlayer => key_dir(KeyCode::W, KeyCode::S),
    };
    let left_dir = match touch_left {
        // drag tracks the pointer 1:1; report direction of travel for the serve check below
        Some(target) => (target - (p.left.y + PADDLE_H / 2.0)).signum(),
        None => left_key_dir,
    };
    p.left = match touch_left {
        Some(target) => set_paddle_center(p.left, target),
        None => move_paddle(p.left, left_key_dir, PADDLE_SPEED, dt),
    };

    let right_dir = match p.mode {
        Mode::VsAi => ai_dir(p.right, p.ball),
        Mode::TwoPlayer => match touch_right {
            Some(target) => (target - (p.right.y + PADDLE_H / 2.0)).signum(),
            None => key_dir(KeyCode::Up, KeyCode::Down),
        },
        Mode::Solo => 0.0,
    };
    if p.mode != Mode::Solo {
        p.right = match (p.mode, touch_right) {
            (Mode::TwoPlayer, Some(target)) => set_paddle_center(p.right, target),
            (Mode::VsAi, _) => move_paddle(p.right, right_dir, AI_SPEED, dt),
            _ => move_paddle(p.right, right_dir, PADDLE_SPEED, dt),
        };
    }

    // Ball rests against the serving paddle until that side moves.
    if let Some(side) = p.serve {
        let (paddle, dir) = match side {
            Side::Left => (p.left, left_dir),
            Side::Right => (p.right, right_dir),
        };
        p.ball = serve_from(paddle, side, dir != 0.0);
        if dir != 0.0 {
            p.serve = None;
        }
        return Screen::Playing(p);
    }

    let right = if p.mode == Mode::Solo { None } else { Some(p.right) };
    let (ball, event) = step_ball(p.ball, p.left, right, dt);
    p.ball = ball;

    match (p.mode, event) {
        (Mode::Solo, BallEvent::Hit) => {
            if p.ball.vx < 0.0 {
                p.score_right += 1; // wall bounce = 1 point
            }
        }
        (Mode::Solo, BallEvent::OutLeft) => {
            if p.score_right > best.solo_bounces {
                best.solo_bounces = p.score_right;
                best.save();
            }
            return Screen::GameOver { message: format!("You survived {} bounces!", p.score_right) };
        }
        (_, BallEvent::OutLeft) => {
            p.score_right += 1;
            if p.mode == Mode::TwoPlayer {
                // right paddle is human here — wait for them to serve
                p.serve = Some(Side::Right);
                p.ball = serve_from(p.right, Side::Right, false);
            } else {
                // AI serves immediately, no one to wait on
                p.ball = serve_from(p.right, Side::Right, true);
            }
        }
        (_, BallEvent::OutRight) => {
            p.score_left += 1;
            p.serve = Some(Side::Left);
            p.ball = serve_from(p.left, Side::Left, false);
        }
        _ => {}
    }

    if p.score_left >= WIN_SCORE || p.score_right >= WIN_SCORE {
        let margin = p.score_left.max(p.score_right) - p.score_left.min(p.score_right);
        let improved = match p.mode {
            Mode::VsAi if p.score_left > p.score_right && margin > best.vs_ai_margin => {
                best.vs_ai_margin = margin;
                true
            }
            Mode::TwoPlayer if margin > best.two_player_margin => {
                best.two_player_margin = margin;
                true
            }
            _ => false,
        };
        if improved {
            best.save();
        }

        let winner = if p.score_left > p.score_right { "Left player" } else { "Right player" };
        let winner = if p.mode == Mode::VsAi && p.score_right > p.score_left { "The AI" } else { winner };
        return Screen::GameOver {
            message: format!(
                "{winner} wins {}-{}",
                p.score_left.max(p.score_right),
                p.score_left.min(p.score_right)
            ),
        };
    }
    Screen::Playing(p)
}

// --- drawing ---------------------------------------------------------------

// The game always simulates a landscape court (game.rs). On portrait screens
// we rotate the view 90° so the left paddle (the human) sits at the bottom
// and the right paddle (the AI) at the top.

fn is_portrait() -> bool {
    screen_height() > screen_width()
}

/// Court coordinates -> screen pixels.
fn to_screen(x: f32, y: f32) -> (f32, f32) {
    if is_portrait() {
        (screen_width() * y / COURT_H, screen_height() * (1.0 - x / COURT_W))
    } else {
        (screen_width() * x / COURT_W, screen_height() * y / COURT_H)
    }
}

/// Screen pixels -> court coordinates (inverse of `to_screen`).
fn to_court(sx: f32, sy: f32) -> (f32, f32) {
    if is_portrait() {
        ((1.0 - sy / screen_height()) * COURT_W, sx / screen_width() * COURT_H)
    } else {
        (sx / screen_width() * COURT_W, sy / screen_height() * COURT_H)
    }
}

/// Draw a court-space rectangle, whatever the orientation.
fn draw_court_rect(x: f32, y: f32, w: f32, h: f32, color: Color) {
    let (x1, y1) = to_screen(x, y);
    let (x2, y2) = to_screen(x + w, y + h);
    draw_rectangle(x1.min(x2), y1.min(y2), (x2 - x1).abs(), (y2 - y1).abs(), color);
}

/// A small outlined button in screen space; returns true when clicked/tapped.
fn button(label: &str, x: f32) -> bool {
    let y = 10.0;
    let dims = measure_text(label, None, 20, 1.0);
    let (w, h) = (dims.width + 20.0, 30.0);
    draw_rectangle_lines(x, y, w, h, 2.0, GRAY);
    draw_text(label, x + 10.0, y + 21.0, 20.0, WHITE);
    let (mx, my) = mouse_position();
    is_mouse_button_pressed(MouseButton::Left) && mx >= x && mx <= x + w && my >= y && my <= y + h
}

fn draw_play(p: &Play) {
    for i in 0..15 {
        draw_court_rect(COURT_W / 2.0 - 3.0, i as f32 * 40.0 + 10.0, 6.0, 20.0, GRAY);
    }

    let right_color = if p.mode == Mode::VsAi { RED } else { ORANGE };
    draw_court_rect(p.left.x, p.left.y, PADDLE_W, PADDLE_H, GREEN);
    if p.mode != Mode::Solo {
        draw_court_rect(p.right.x, p.right.y, PADDLE_W, PADDLE_H, right_color);
    }

    let (bx, by) = to_screen(p.ball.x, p.ball.y);
    draw_circle(bx, by, BALL_R * screen_width().min(screen_height()) / COURT_H, WHITE);

    let score = match p.mode {
        Mode::Solo => format!("Bounces: {}", p.score_right),
        _ => format!("{}   {}", p.score_left, p.score_right),
    };
    let dims = measure_text(&score, None, 48, 1.0);
    draw_text(&score, (screen_width() - dims.width) / 2.0, 60.0, 48.0, WHITE);

    // legend: colored names under the matching score numbers
    let legend = match p.mode {
        Mode::VsAi => Some(("YOU", "AI")),
        Mode::TwoPlayer => Some(("P1", "P2")),
        Mode::Solo => None,
    };
    if let Some((left_name, right_name)) = legend {
        let center = screen_width() / 2.0;
        let ld = measure_text(left_name, None, 18, 1.0);
        let rd = measure_text(right_name, None, 18, 1.0);
        draw_text(left_name, center - 40.0 - ld.width / 2.0, 90.0, 18.0, GREEN);
        draw_text(right_name, center + 40.0 - rd.width / 2.0, 90.0, 18.0, right_color);
    }
}

/// Line spacing for a `count`-line centered block, shrunk so the whole
/// block always fits short screens (phone landscape).
fn line_spacing(count: usize) -> f32 {
    (screen_height() / (count as f32 + 1.0)).min(50.0)
}

/// Baseline y of line `i`; shared by drawing and tap hit-testing so they
/// can't drift apart.
fn line_y(count: usize, i: usize) -> f32 {
    let spacing = line_spacing(count);
    screen_height() / 2.0 - (count as f32 - 1.0) / 2.0 * spacing + i as f32 * spacing
}

fn draw_centered_lines(lines: &[&str]) {
    // shrink to fit the widest line on narrow (portrait) screens
    let widest = lines
        .iter()
        .map(|l| measure_text(l, None, 40, 1.0).width)
        .fold(1.0_f32, f32::max);
    let font_size = (line_spacing(lines.len()) * 0.8)
        .min(40.0)
        .min(40.0 * screen_width() * 0.95 / widest);
    for (i, line) in lines.iter().enumerate() {
        let dims = measure_text(line, None, font_size as u16, 1.0);
        draw_text(line, (screen_width() - dims.width) / 2.0, line_y(lines.len(), i), font_size, WHITE);
    }
}

/// Index of the line tapped this frame, if any. Touch taps arrive as
/// simulated mouse clicks in macroquad, so this covers mouse and finger.
fn tapped_line(count: usize) -> Option<usize> {
    if !is_mouse_button_pressed(MouseButton::Left) {
        return None;
    }
    let my = mouse_position().1;
    (0..count).find(|&i| (my - line_y(count, i)).abs() < line_spacing(count) / 2.0)
}

#[macroquad::main("Bing Pong")]
async fn main() {
    let mut screen = Screen::Menu;
    let mut best = BestScores::load();

    loop {
        clear_background(BLACK);
        let dt = get_frame_time().min(0.05); // ponytail: clamp dt so a hung tab can't teleport the ball

        screen = match screen {
            Screen::Menu => {
                let lines = [
                    "BING PONG",
                    "",
                    "1 - Play vs AI",
                    "2 - Two players (W/S vs Up/Down)",
                    "3 - Solo wall-bounce",
                    "",
                    &format!(
                        "Best: vs AI +{}   2P +{}   Solo {}",
                        best.vs_ai_margin, best.two_player_margin, best.solo_bounces
                    ),
                    "",
                    "Tap an option or press its number",
                ];
                draw_centered_lines(&lines);
                let tapped = tapped_line(lines.len());
                if is_key_pressed(KeyCode::Key1) || tapped == Some(2) {
                    Screen::Playing(Play::new(Mode::VsAi))
                } else if is_key_pressed(KeyCode::Key2) || tapped == Some(3) {
                    Screen::Playing(Play::new(Mode::TwoPlayer))
                } else if is_key_pressed(KeyCode::Key3) || tapped == Some(4) {
                    Screen::Playing(Play::new(Mode::Solo))
                } else {
                    Screen::Menu
                }
            }
            Screen::Playing(p) => {
                let next = if p.paused { Screen::Playing(p) } else { update_play(p, dt, &mut best) };
                match next {
                    Screen::Playing(mut p) => {
                        draw_play(&p);
                        if p.paused {
                            let dims = measure_text("Paused", None, 40, 1.0);
                            draw_text("Paused", (screen_width() - dims.width) / 2.0, screen_height() / 2.0 - 20.0, 40.0, WHITE);
                        }
                        if button(if p.paused { "Resume" } else { "Pause" }, 80.0) || is_key_pressed(KeyCode::Escape) {
                            p.paused = !p.paused;
                        }
                        if button("Exit", 10.0) {
                            Screen::Menu
                        } else {
                            Screen::Playing(p)
                        }
                    }
                    other => other,
                }
            }
            Screen::GameOver { message } => {
                draw_centered_lines(&[&message, "", "Tap or press SPACE for menu"]);
                if is_key_pressed(KeyCode::Space) || is_mouse_button_pressed(MouseButton::Left) {
                    Screen::Menu
                } else {
                    Screen::GameOver { message }
                }
            }
        };

        next_frame().await
    }
}
