// Messages that are not errors (counts, tallies, warnings), suppressed by -q
// (plan/step16.txt decision 7). The flag is set once, from the options, before
// anything is read; a process-wide flag spares passing it to every module that
// reports something.

use std::sync::atomic::{AtomicBool, Ordering};

static QUIET: AtomicBool = AtomicBool::new(false);

pub fn set_quiet(quiet: bool) {
    QUIET.store(quiet, Ordering::Relaxed);
}

pub fn quiet() -> bool {
    QUIET.load(Ordering::Relaxed)
}

// note!(...) is eprintln!(...) unless -q was given.
macro_rules! note {
    ($($arg:tt)*) => {
        if !crate::note::quiet() {
            eprintln!($($arg)*);
        }
    };
}
pub(crate) use note;
