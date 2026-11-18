//! Groups behavior tests for the adjacent provider or model.

use super::*;
use base64::Engine;
use quotaline_quota::NoticeKind;
use quotaline_quota::ResetPrecision;
use quotaline_text::text::humanize;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers};

mod shared_helpers;
use shared_helpers::*;
mod claude_helpers;
use claude_helpers::*;
mod antigravity_helpers;
use antigravity_helpers::*;
mod cursor_helpers;
use cursor_helpers::*;
mod codex_helpers;
use codex_helpers::*;
mod copilot_helpers;
use copilot_helpers::*;
mod gemini_helpers;
use gemini_helpers::*;

mod antigravity_1;
mod antigravity_2;
mod antigravity_3;
mod claude_1;
mod claude_2;
mod claude_3;
mod codex_1;
mod codex_2;
mod codex_3;
mod codex_4;
mod copilot_1;
mod copilot_2;
mod cursor_1;
mod cursor_2;
mod gemini_1;
mod gemini_2;
mod shared_1;
mod shared_2;
mod transport;
