//! Pure game logic — no macroquad types, so it all runs under `cargo test`.

pub const COURT_W: f32 = 800.0;
pub const COURT_H: f32 = 600.0;
pub const PADDLE_W: f32 = 14.0;
pub const PADDLE_H: f32 = 100.0;
pub const BALL_R: f32 = 10.0;
pub const PADDLE_SPEED: f32 = 600.0;
pub const AI_SPEED: f32 = 300.0;
pub const BALL_SPEED: f32 = 360.0;
pub const SPEEDUP_ON_HIT: f32 = 1.04;

#[derive(Clone, Copy, PartialEq)]
pub struct Ball {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
}

#[derive(Clone, Copy, PartialEq)]
pub struct Paddle {
    pub x: f32,
    pub y: f32, // top edge
}

impl Paddle {
    pub fn new(x: f32) -> Paddle {
        Paddle { x, y: (COURT_H - PADDLE_H) / 2.0 }
    }
}

/// Which paddle is serving.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Side {
    Left,
    Right,
}

/// Ball resting against the serving paddle. `launch` false holds it there
/// (vx=vy=0) tracking the paddle each frame; true fires it toward the court.
pub fn serve_from(paddle: Paddle, side: Side, launch: bool) -> Ball {
    let dir = match side {
        Side::Left => 1.0,
        Side::Right => -1.0,
    };
    let x = match side {
        Side::Left => paddle.x + PADDLE_W + BALL_R,
        Side::Right => paddle.x - BALL_R,
    };
    Ball {
        x,
        y: paddle.y + PADDLE_H / 2.0,
        vx: if launch { BALL_SPEED * dir } else { 0.0 },
        vy: if launch { BALL_SPEED * 0.5 } else { 0.0 },
    }
}

/// What happened to the ball this frame.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum BallEvent {
    None,
    Hit,      // bounced off a paddle or (in solo mode) the back wall
    OutLeft,  // left player missed
    OutRight, // right player missed
}

/// Move a paddle by `dir` (-1.0 up, 1.0 down, 0.0 hold), clamped to the court.
pub fn move_paddle(p: Paddle, dir: f32, speed: f32, dt: f32) -> Paddle {
    let y = (p.y + dir * speed * dt).clamp(0.0, COURT_H - PADDLE_H);
    Paddle { y, ..p }
}

/// Snap a paddle so its center sits at `target_center_y`, clamped to the court.
/// Used for touch/mouse drag, which should track the pointer 1:1 instead of
/// chasing it at a capped speed.
pub fn set_paddle_center(p: Paddle, target_center_y: f32) -> Paddle {
    let y = (target_center_y - PADDLE_H / 2.0).clamp(0.0, COURT_H - PADDLE_H);
    Paddle { y, ..p }
}

/// AI: track the ball's center with capped speed.
pub fn ai_dir(p: Paddle, ball: Ball) -> f32 {
    let center = p.y + PADDLE_H / 2.0;
    let diff = ball.y - center;
    // ponytail: dead zone stops jitter; smarter prediction not needed for Pong
    if diff.abs() < 12.0 { 0.0 } else { diff.signum() }
}

fn hits_paddle(ball: Ball, p: Paddle) -> bool {
    ball.y + BALL_R >= p.y
        && ball.y - BALL_R <= p.y + PADDLE_H
        && ball.x + BALL_R >= p.x
        && ball.x - BALL_R <= p.x + PADDLE_W
}

/// Advance the ball one frame against two paddles. `right` is None in solo
/// mode, where the right wall bounces instead of scoring.
pub fn step_ball(ball: Ball, left: Paddle, right: Option<Paddle>, dt: f32) -> (Ball, BallEvent) {
    let mut b = Ball { x: ball.x + ball.vx * dt, y: ball.y + ball.vy * dt, ..ball };

    // top/bottom walls
    if b.y - BALL_R <= 0.0 {
        b.y = BALL_R;
        b.vy = b.vy.abs();
    } else if b.y + BALL_R >= COURT_H {
        b.y = COURT_H - BALL_R;
        b.vy = -b.vy.abs();
    }

    if b.vx < 0.0 && hits_paddle(b, left) {
        b.x = left.x + PADDLE_W + BALL_R;
        b.vx = b.vx.abs() * SPEEDUP_ON_HIT;
        return (b, BallEvent::Hit);
    }
    if let Some(r) = right {
        if b.vx > 0.0 && hits_paddle(b, r) {
            b.x = r.x - BALL_R;
            b.vx = -b.vx.abs() * SPEEDUP_ON_HIT;
            return (b, BallEvent::Hit);
        }
    } else if b.x + BALL_R >= COURT_W {
        // solo mode: right wall is a bouncer
        b.x = COURT_W - BALL_R;
        b.vx = -b.vx.abs();
        return (b, BallEvent::Hit);
    }

    if b.x + BALL_R < 0.0 {
        return (b, BallEvent::OutLeft);
    }
    if b.x - BALL_R > COURT_W {
        return (b, BallEvent::OutRight);
    }
    (b, BallEvent::None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serve_from_holds_until_launched() {
        let p = Paddle::new(20.0);
        let held = serve_from(p, Side::Left, false);
        assert_eq!((held.vx, held.vy), (0.0, 0.0));

        let launched = serve_from(p, Side::Left, true);
        assert!(launched.vx > 0.0, "left serve should launch rightward");

        let launched_right = serve_from(Paddle::new(776.0), Side::Right, true);
        assert!(launched_right.vx < 0.0, "right serve should launch leftward");
    }

    #[test]
    fn ball_bounces_off_top_wall() {
        // Arrange
        let ball = Ball { x: 400.0, y: BALL_R + 1.0, vx: 100.0, vy: -200.0 };
        // Act
        let (after, _) = step_ball(ball, Paddle::new(10.0), Some(Paddle::new(776.0)), 0.016);
        // Assert
        assert!(after.vy > 0.0, "vy should flip to downward");
    }

    #[test]
    fn ball_bounces_off_left_paddle_and_speeds_up() {
        let left = Paddle::new(10.0);
        let ball = Ball { x: left.x + PADDLE_W + BALL_R + 1.0, y: left.y + 50.0, vx: -300.0, vy: 0.0 };
        let (after, event) = step_ball(ball, left, Some(Paddle::new(776.0)), 0.016);
        assert_eq!(event, BallEvent::Hit);
        assert!(after.vx > 300.0 * (SPEEDUP_ON_HIT - 0.001));
    }

    #[test]
    fn ball_past_left_edge_scores_for_right() {
        let ball = Ball { x: -BALL_R - 1.0, y: 300.0, vx: -300.0, vy: 0.0 };
        let (_, event) = step_ball(ball, Paddle::new(10.0), Some(Paddle::new(776.0)), 0.016);
        assert_eq!(event, BallEvent::OutLeft);
    }

    #[test]
    fn solo_mode_right_wall_bounces_instead_of_scoring() {
        let ball = Ball { x: COURT_W - BALL_R - 1.0, y: 300.0, vx: 300.0, vy: 0.0 };
        let (after, event) = step_ball(ball, Paddle::new(10.0), None, 0.016);
        assert_eq!(event, BallEvent::Hit);
        assert!(after.vx < 0.0);
    }

    #[test]
    fn paddle_clamped_to_court() {
        let p = move_paddle(Paddle::new(10.0), -1.0, PADDLE_SPEED, 100.0);
        assert_eq!(p.y, 0.0);
        let p = move_paddle(p, 1.0, PADDLE_SPEED, 100.0);
        assert_eq!(p.y, COURT_H - PADDLE_H);
    }

    #[test]
    fn ai_holds_still_inside_dead_zone_and_tracks_outside() {
        let p = Paddle::new(776.0);
        let center = p.y + PADDLE_H / 2.0;
        let near = Ball { x: 400.0, y: center + 5.0, vx: 0.0, vy: 0.0 };
        let far = Ball { x: 400.0, y: center + 100.0, vx: 0.0, vy: 0.0 };
        assert_eq!(ai_dir(p, near), 0.0);
        assert_eq!(ai_dir(p, far), 1.0);
    }
}
