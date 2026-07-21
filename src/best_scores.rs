//! Persisted high scores: a browser tab keeps them in localStorage (via a small
//! JS plugin appended to mq_js_bundle.js), a native build keeps them in a file.

#[cfg(target_arch = "wasm32")]
mod persist {
    unsafe extern "C" {
        fn best_scores_save(ptr: *const u8, len: u32);
        fn best_scores_load(out_ptr: *mut u8, max_len: u32) -> u32;
    }

    pub fn save(s: &str) {
        unsafe { best_scores_save(s.as_ptr(), s.len() as u32) }
    }

    pub fn load() -> String {
        let mut buf = [0u8; 64];
        let n = unsafe { best_scores_load(buf.as_mut_ptr(), buf.len() as u32) };
        String::from_utf8_lossy(&buf[..n as usize]).into_owned()
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod persist {
    const PATH: &str = "best_scores.txt";

    pub fn save(s: &str) {
        let _ = std::fs::write(PATH, s);
    }

    pub fn load() -> String {
        std::fs::read_to_string(PATH).unwrap_or_default()
    }
}

#[derive(Clone, Copy, Default)]
pub struct BestScores {
    pub solo_bounces: u32,
    pub vs_ai_margin: u32,
    pub two_player_margin: u32,
}

impl BestScores {
    pub fn load() -> BestScores {
        let raw = persist::load();
        let mut parts = raw.split(',').map(|s| s.trim().parse().unwrap_or(0));
        BestScores {
            solo_bounces: parts.next().unwrap_or(0),
            vs_ai_margin: parts.next().unwrap_or(0),
            two_player_margin: parts.next().unwrap_or(0),
        }
    }

    pub fn save(&self) {
        persist::save(&format!("{},{},{}", self.solo_bounces, self.vs_ai_margin, self.two_player_margin));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_then_load_round_trips() {
        let best = BestScores { solo_bounces: 12, vs_ai_margin: 3, two_player_margin: 5 };
        best.save();
        let loaded = BestScores::load();
        assert_eq!((loaded.solo_bounces, loaded.vs_ai_margin, loaded.two_player_margin), (12, 3, 5));
    }
}
