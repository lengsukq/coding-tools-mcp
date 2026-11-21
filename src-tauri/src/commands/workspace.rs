use std::path::PathBuf;
#[cfg(not(target_os = "windows"))]
use std::process::Command;

use tauri::{AppHandle, Manager, State};

use crate::app_state::AppState;
use crate::error::{AppError, AppResult};
use crate::platform::{background_command, open_path_in_file_manager};
use crate::workspace::WorkspaceProfile;

mod crud;
mod git;
mod ide;
mod reviews;

pub use crud::*;
pub use git::*;
pub use ide::*;
pub use reviews::*;
