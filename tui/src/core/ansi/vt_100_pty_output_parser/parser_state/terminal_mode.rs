// Copyright (c) 2022-2025 R3BL LLC. Licensed under Apache License, Version 2.0.

use super::super::modes::{MouseTrackingFormat, MouseTrackingMode,
                          terminal_mode_state_todo};
use crate::{ActiveScreenBuffer, CursorKeyMode};

/// State tracking for terminal operational modes.
///
/// Used by the [`VT-100`] [`ANSI`] parser performer ([`AnsiToOfsBufPerformer`])
/// to maintain state information about the operational modes requested by the
/// underlying [`PTY`] process.
///
/// [`ANSI`]: https://en.wikipedia.org/wiki/ANSI_escape_code
/// [`AnsiToOfsBufPerformer`]: crate::AnsiToOfsBufPerformer
/// [`PTY`]: https://en.wikipedia.org/wiki/Pseudoterminal
/// [`VT-100`]: https://vt100.net/docs/vt100-ug/chapter3.html
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TerminalModeState {
    /// Cursor key mode status ([`DECCKM`]).
    ///
    /// Controls whether cursor keys (arrows, home, end) send normal or application
    /// escape sequences.
    ///
    /// Toggled by the [`AnsiToOfsBufPerformer`] when processing the `ESC [ ? 1 h` and
    /// `ESC [ ? 1 l` sequences.
    ///
    /// [`AnsiToOfsBufPerformer`]: crate::AnsiToOfsBufPerformer
    /// [`DECCKM`]: https://vt100.net/docs/vt100-ug/chapter3.html#DECCKM
    pub cursor_key_mode: CursorKeyMode,

    /// Alternate screen buffer status.
    ///
    /// When active, terminal output is redirected to an alternate screen buffer,
    /// preserving the original screen content.
    ///
    /// Toggled by the [`AnsiToOfsBufPerformer`] when processing the `ESC [ ? 1049 h`
    /// and `ESC [ ? 1049 l` sequences.
    ///
    /// [`AnsiToOfsBufPerformer`]: crate::AnsiToOfsBufPerformer
    pub active_screen_buffer: ActiveScreenBuffer,

    /// Mouse tracking enabled/disabled state.
    pub mouse_tracking_mode: MouseTrackingMode,

    /// Mouse tracking encoding format requested by the app - [X10] or [Sgr].
    ///
    /// See the [implementation note] in [`MouseTrackingFormat`] for exact details on how
    /// events are routed and formatted based on the app's requested protocols.
    ///
    /// [`MouseTrackingFormat`]: crate::MouseTrackingFormat
    /// [implementation note]: crate::MouseTrackingFormat#implementation-note
    /// [Sgr]: MouseTrackingFormat::Sgr
    /// [X10]: MouseTrackingFormat::X10
    pub mouse_tracking_format: MouseTrackingFormat,

    /// Bracketed paste mode status (DECSET/DECRST 2004).
    ///
    /// Updated by [`vt_100_shim_mode_ops`] when processing `CSI ? 2004 h` / `l`.
    /// When [`Enabled`], the app wraps pasted text in `\e[200~` and `\e[201~`
    /// sequences so the PTY child can distinguish pasted input from typed input.
    ///
    /// [`Enabled`]: terminal_mode_state_todo::BracketedPasteMode::Enabled
    /// [`vt_100_shim_mode_ops`]: crate::core::ansi::vt_100_pty_output_parser::ops::vt_100_shim_mode_ops
    pub bracketed_paste: terminal_mode_state_todo::BracketedPasteMode,

    /// Focus event reporting mode.
    ///
    /// When enabled (by `CSI ? 1004 h`), the PTY child wants to receive
    /// `CSI I` / `CSI O` focus events from the terminal multiplexer.
    pub focus_events: bool,

    /// Synchronized output mode (DEC private mode 2026).
    ///
    /// When enabled, the terminal should defer rendering until the mode is
    /// reset, allowing atomic screen updates.
    pub synchronized_output: bool,
}

impl TerminalModeState {
    /// Whether the PTY child has enabled bracketed paste mode (DECSET 2004).
    pub fn is_bracketed_paste_enabled(&self) -> bool {
        self.bracketed_paste
            == terminal_mode_state_todo::BracketedPasteMode::Enabled
    }
}
