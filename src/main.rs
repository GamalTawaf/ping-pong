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

/// Advance one frame; returns the next screen.
fn update_play(mut p: Play, dt: f32, best: &mut BestScores) -> Screen {
    let left_dir = match p.mode {
        // one human plays the left paddle alone, so accept either key layout
        Mode::VsAi | Mode::Solo => (key_dir(KeyCode::W, KeyCode::S) + key_dir(KeyCode::Up, KeyCode::Down)).clamp(-1.0, 1.0),
        Mode::TwoPlayer => key_dir(KeyCode::W, KeyCode::S),
    };
    p.left = move_paddle(p.left, left_dir, PADDLE_SPEED, dt);

    let right_dir = match p.mode {
        Mode::VsAi => ai_dir(p.right, p.ball),
        Mode::TwoPlayer => key_dir(KeyCode::Up, KeyCode::Down),
        Mode::Solo => 0.0,
    };
    if p.mode != Mode::Solo {
        let speed = if p.mode == Mode::VsAi { AI_SPEED } else { PADDLE_SPEED };
        p.right = move_paddle(p.right, right_dir, speed, dt);
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

/// Scale court coordinates to the actual window/canvas size.
fn scale() -> (f32, f32) {
    (screen_width() / COURT_W, screen_height() / COURT_H)
}

fn draw_play(p: &Play) {
    let (sx, sy) = scale();
    for i in 0..15 {
        draw_rectangle(screen_width() / 2.0 - 2.0, i as f32 * 40.0 * sy + 10.0, 4.0, 20.0 * sy, GRAY);
    }
    draw_rectangle(p.left.x * sx, p.left.y * sy, PADDLE_W * sx, PADDLE_H * sy, WHITE);
    if p.mode != Mode::Solo {
        draw_rectangle(p.right.x * sx, p.right.y * sy, PADDLE_W * sx, PADDLE_H * sy, WHITE);
    }
    draw_circle(p.ball.x * sx, p.ball.y * sy, BALL_R * sx.min(sy), WHITE);

    let score = match p.mode {
        Mode::Solo => format!("Bounces: {}", p.score_right),
        _ => format!("{}   {}", p.score_left, p.score_right),
    };
    let dims = measure_text(&score, None, 48, 1.0);
    draw_text(&score, (screen_width() - dims.width) / 2.0, 60.0, 48.0, WHITE);
}

fn draw_centered_lines(lines: &[&str]) {
    let mut y = screen_height() / 2.0 - lines.len() as f32 * 25.0;
    for line in lines {
        let dims = measure_text(line, None, 40, 1.0);
        draw_text(line, (screen_width() - dims.width) / 2.0, y, 40.0, WHITE);
        y += 50.0;
    }
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
                draw_centered_lines(&[
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
                ]);
                if is_key_pressed(KeyCode::Key1) {
                    Screen::Playing(Play::new(Mode::VsAi))
                } else if is_key_pressed(KeyCode::Key2) {
                    Screen::Playing(Play::new(Mode::TwoPlayer))
                } else if is_key_pressed(KeyCode::Key3) {
                    Screen::Playing(Play::new(Mode::Solo))
                } else {
                    Screen::Menu
                }
            }
            Screen::Playing(p) => {
                let next = update_play(p, dt, &mut best);
                if let Screen::Playing(ref p) = next {
                    draw_play(p);
                }
                next
            }
            Screen::GameOver { message } => {
                draw_centered_lines(&[&message, "", "Press SPACE for menu"]);
                if is_key_pressed(KeyCode::Space) {
                    Screen::Menu
                } else {
                    Screen::GameOver { message }
                }
            }
        };

        next_frame().await
    }
}
