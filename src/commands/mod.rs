mod daemon;
mod print_edids;
pub use daemon::{check, daemon};
pub use print_edids::main as print_edids;
