use std::mem::size_of;
use chrono::{DateTime, Duration, Utc};
use tokio::task::JoinHandle;
use tokio::time;

use windows::Win32::System::SystemInformation::GetTickCount;
use winapi::um::winuser::{GetLastInputInfo, LASTINPUTINFO};

pub fn get_last_input() -> Duration {
    let tick_count = unsafe { GetTickCount() };
    let mut last_input_info = LASTINPUTINFO {
        cbSize: size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };

    let p_last_input_info = &mut last_input_info as *mut LASTINPUTINFO;
    let _ = unsafe { GetLastInputInfo(p_last_input_info) };
    let diff = tick_count.saturating_sub(last_input_info.dwTime);
    Duration::milliseconds(diff as i64)
}

// Tronca a secondi interi per evitare che il jitter sui millisecondi faccia
// oscillare rapidamente lo stato idle intorno alla soglia.
fn polled_idle_duration() -> Duration {
    Duration::seconds(get_last_input().num_seconds())
}

const IDLE_CHECK_SECS: u64 = 1; // 1 second
// Un salto dell'orologio di sistema molto più grande dell'intervallo di
// polling non può essere spiegato da normale jitter dello scheduler: significa
// che Windows è stato sospeso. GetTickCount/GetLastInputInfo si "congelano"
// durante la sospensione, quindi non possiamo affidarci ad essi per rilevarla.

// Macchina a stati del rilevamento inattività/sospensione, isolata dalle
// chiamate di sistema reali e dal timer async in modo da poterla testare.
struct IdleWatcherState {
    was_idle: bool,
    idle_start: Option<DateTime<Utc>>,
    last_check: DateTime<Utc>,
    last_idle_duration: Duration,
}

impl IdleWatcherState {
    fn new(now: DateTime<Utc>) -> Self {
        Self {
            was_idle: false,
            idle_start: None,
            last_check: now,
            last_idle_duration: Duration::zero(),
        }
    }

    // Elabora un singolo tick di polling. `get_idle` viene interrogato solo
    // quando non si è già rilevato un salto dell'orologio dovuto a una
    // sospensione. Ritorna Some(durata) quando l'utente torna attivo dopo un
    // periodo di inattività o sospensione, cioè la durata da segnalare come
    // tempo offline.
    fn on_tick(
        &mut self,
        now: DateTime<Utc>,
        idle_period: Duration,
        get_idle: impl FnOnce() -> Duration,
    ) -> Option<Duration> {
        let wall_gap = (now - self.last_check) + self.last_idle_duration;
        let previous_check = self.last_check;
        self.last_check = now;

        if !self.was_idle && wall_gap >= idle_period {
            // Ripresa da sospensione mentre l'utente non era già rilevato
            // come inattivo: l'ultimo momento di reale attività è quello
            // precedente al salto di orologio, non "adesso".
            self.was_idle = true;
            self.idle_start = Some(previous_check - self.last_idle_duration);
            return None;
        }

        let idle_duration = get_idle();
        self.last_idle_duration = idle_duration;

        if idle_duration >= idle_period {
            if !self.was_idle {
                self.was_idle = true;
                self.idle_start = Some(now - idle_duration);
            }
            None
        } else if self.was_idle {
            let offline_duration = self
                .idle_start
                .map(|start| now - start)
                .unwrap_or(idle_duration);
            self.was_idle = false;
            self.idle_start = None;
            Some(offline_duration)
        } else {
            None
        }
    }
}

pub fn start_idle_watcher(
    idle_return_tx: tokio::sync::mpsc::UnboundedSender<Duration>,
    idle_period_secs: u64,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let idle_period = Duration::seconds(idle_period_secs as i64);
        let mut interval = time::interval(std::time::Duration::from_secs(IDLE_CHECK_SECS));
        // Evita di "raffica-recuperare" migliaia di tick mancati dopo una
        // sospensione lunga: al risveglio scatta un solo tick.
        interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
        let mut state = IdleWatcherState::new(Utc::now());

        loop {
            interval.tick().await;
            let now = Utc::now();
            if let Some(offline_duration) = state.on_tick(now, idle_period, polled_idle_duration) {
                let _ = idle_return_tx.send(offline_duration);
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(base: DateTime<Utc>, offset_secs: i64) -> DateTime<Utc> {
        base + Duration::seconds(offset_secs)
    }

    #[test]
    fn stays_active_while_input_keeps_arriving() {
        let base = Utc::now();
        let idle_period = Duration::seconds(60);
        let mut state = IdleWatcherState::new(base);

        for t in 1..=5 {
            let emitted = state.on_tick(at(base, t), idle_period, Duration::zero);
            assert_eq!(emitted, None);
        }
        assert!(!state.was_idle);
    }

    #[test]
    fn detects_idle_and_reports_duration_on_resume() {
        let base = Utc::now();
        let idle_period = Duration::seconds(60);
        let mut state = IdleWatcherState::new(base);

        // L'utente smette di interagire: dopo 90s l'OS riporta 90s di inattività.
        let emitted = state.on_tick(at(base, 90), idle_period, || Duration::seconds(90));
        assert_eq!(emitted, None);
        assert!(state.was_idle);

        // Un secondo dopo l'utente torna attivo (idle_duration azzerata).
        let emitted = state.on_tick(at(base, 91), idle_period, Duration::zero);
        assert_eq!(emitted, Some(Duration::seconds(91)));
        assert!(!state.was_idle);
    }

    #[test]
    fn does_not_flag_idle_below_threshold() {
        let base = Utc::now();
        let idle_period = Duration::seconds(60);
        let mut state = IdleWatcherState::new(base);

        let emitted = state.on_tick(at(base, 30), idle_period, || Duration::seconds(30));
        assert_eq!(emitted, None);
        assert!(!state.was_idle);
    }

    #[test]
    fn suspend_gap_is_reported_in_full_once_user_returns() {
        // Riproduce una sospensione di 5 ore: GetTickCount/GetLastInputInfo
        // sono congelati, ma l'orologio di sistema salta in avanti.
        let base = Utc::now();
        let idle_period = Duration::seconds(60);
        let mut state = IdleWatcherState::new(base);

        // Tick regolare poco prima della sospensione: utente idle da 2s.
        let before_sleep = at(base, 5);
        let emitted = state.on_tick(before_sleep, idle_period, || Duration::seconds(2));
        assert_eq!(emitted, None);
        assert!(!state.was_idle);

        // Il PC si sospende qui e si risveglia 5 ore dopo: il primo tick
        // post-risveglio vede un salto enorme dell'orologio.
        let resume = at(before_sleep, 5 * 60 * 60);
        let emitted = state.on_tick(resume, idle_period, || {
            panic!("get_idle non deve essere chiamato quando si rileva un salto di orologio")
        });
        assert_eq!(emitted, None);
        assert!(state.was_idle);

        // Un secondo dopo il risveglio l'utente muove il mouse.
        let after_resume = at(resume, 1);
        let emitted = state.on_tick(after_resume, idle_period, Duration::zero);

        // La durata segnalata deve coprire (quasi) l'intera sospensione, non
        // solo i secondi trascorsi dal risveglio.
        let reported = emitted.expect("deve segnalare il tempo offline");
        assert!(
            reported >= Duration::hours(4) + Duration::minutes(59),
            "reported duration was {reported:?}, expected ~5 hours"
        );
    }

    #[test]
    fn still_idle_after_suspend_detection_waits_for_real_activity() {
        let base = Utc::now();
        let idle_period = Duration::seconds(60);
        let mut state = IdleWatcherState::new(base);

        let resume = at(base, 5 * 60 * 60);
        state.on_tick(resume, idle_period, Duration::zero);
        assert!(state.was_idle);

        // L'utente è ancora lontano dal computer dopo il risveglio.
        let emitted = state.on_tick(at(resume, 1), idle_period, || Duration::seconds(120));
        assert_eq!(emitted, None);
        assert!(state.was_idle);
    }
}
