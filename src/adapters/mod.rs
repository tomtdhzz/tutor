//! Adapters: infrastructure implementing the app ports.
//! The omp session store, the `omp -p` brain, a file-backed deck cache, and the
//! system clock. All omp/JSON/filesystem knowledge is confined here.

pub mod cache_file;
pub mod clock;
pub mod omp_sessions;
pub mod omp_summarize;

pub use cache_file::FileDeckStore;
pub use clock::SystemClock;
pub use omp_sessions::OmpSessions;
pub use omp_summarize::OmpSummarizer;
