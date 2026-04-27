# LLD: tmux Log Cleaning (ANSI/VT100 Strip)

**Milestone:** M35  
**Status:** draft  
**HLD ref:** §4.3, §8  
**Implements:** `crates/aegis-tmux/` — new `log_clean` module

---

## 1. Purpose

`tmux pipe-pane` captures the raw byte stream written to a pane's pseudo-terminal and appends it verbatim to a log file via `cat >>`. Because the agent CLIs (claude, gemini, etc.) run inside a terminal emulator, their output contains ANSI/VT100 control sequences: SGR colour codes, cursor-movement sequences, erase-line/erase-screen commands, and OSC sequences. These make the log files difficult to read and pollute the failover context window injected into replacement agents.

This milestone adds a canonical `log_clean` module to `crates/aegis-tmux/` that strips all terminal control sequences and normalises whitespace. All log consumers — `aegis-recorder`'s `query()` path and `aegis-controller`'s `LogTailer` — must route through this module so cleaning logic is not duplicated.

---

## 2. Module Structure

One new file is added to the existing crate; no other source files change structurally.

```
crates/aegis-tmux/
├── Cargo.toml          ← add strip-ansi-escapes dependency
└── src/
    ├── lib.rs          ← re-export log_clean::{strip_log_line, clean_log}
    ├── client.rs       (unchanged)
    ├── escape.rs       (unchanged)
    ├── target.rs       (unchanged)
    ├── error.rs        (unchanged)
    └── log_clean.rs    ← NEW
```

`aegis-controller`'s direct dependency on `strip-ansi-escapes` is removed; it calls `aegis_tmux::strip_log_line` instead.

---

## 3. Data Model

No new persistent types. The module operates on borrowed string slices and returns owned `String` values. There is no state.

### 3.1 Sequence categories stripped

| Category | Examples |
|---|---|
| CSI sequences | `\x1b[0m`, `\x1b[31m`, `\x1b[2J`, `\x1b[H`, `\x1b[1;32m` |
| OSC sequences | `\x1b]0;title\x07`, `\x1b]2;...\x1b\\` |
| DCS / PM / APC | `\x1bP...`, `\x1bX...` |
| Single-char escapes | `\x1b[ABCD` (cursor keys), `\x1bc` (RIS), `\x1b=` |
| Standalone `\r` | carriage returns without following `\n` |
| Null bytes | `\x00` |

### 3.2 Whitespace normalisation

After stripping control sequences, each line is processed as follows:

1. Strip leading and trailing whitespace (spaces, tabs).
2. Collapse interior runs of two or more spaces to a single space. This removes the padding left by erase-to-end-of-line sequences that were replaced with spaces after CSI stripping.
3. Lines that are empty after normalisation are treated as blank lines — they are preserved in `clean_log` output as empty strings but are not emitted from `strip_log_line`.

---

## 4. Public API

```rust
// crates/aegis-tmux/src/log_clean.rs

/// Strip ANSI/VT100 control sequences and normalise whitespace from a single
/// input line. Returns an empty string for lines that are blank after stripping.
///
/// Suitable for per-line processing by callers that already split on newlines.
pub fn strip_log_line(line: &str) -> String;

/// Process a raw pipe-pane log chunk (may contain multiple lines and `\r\n`
/// sequences). Returns a `\n`-separated string with all control sequences
/// stripped and whitespace normalised.
///
/// This is the entry point for bulk operations (e.g. initial log burst in
/// LogTailer, failover context assembly in the Watchdog).
pub fn clean_log(raw: &str) -> String;
```

### 4.1 Re-exports from `lib.rs`

```rust
pub use log_clean::{clean_log, strip_log_line};
```

### 4.2 Call sites to update

| Crate | File | Current call | Replace with |
|---|---|---|---|
| `aegis-controller` | `src/daemon/logs.rs` | `strip_ansi_escapes::strip(line)` | `aegis_tmux::strip_log_line(line)` |
| `aegis-recorder` | `src/query.rs` | none (raw lines returned) | wrap each line from `tail_lines` / `read_all_lines` with `aegis_tmux::strip_log_line` |

---

## 5. Implementation

### 5.1 Dependency

```toml
# crates/aegis-tmux/Cargo.toml
[dependencies]
strip-ansi-escapes = "0.2"
```

`strip-ansi-escapes` implements a VTE-compliant parser that handles all standard sequence types including multi-byte CSI parameters, OSC with both `BEL` and `ST` terminators, and incomplete sequences at buffer boundaries.

### 5.2 `strip_log_line`

```rust
pub fn strip_log_line(line: &str) -> String {
    // 1. Strip ANSI/VT100 sequences via VTE parser.
    let stripped = strip_ansi_escapes::strip(line);
    let text = String::from_utf8_lossy(&stripped);

    // 2. Remove isolated carriage returns and null bytes.
    let text = text.replace('\r', "").replace('\x00', "");

    // 3. Trim leading/trailing whitespace.
    let text = text.trim();

    // 4. Collapse interior double-spaces (artifact of erased terminal cells).
    collapse_spaces(text)
}

fn collapse_spaces(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars() {
        if ch == ' ' {
            if !prev_space {
                out.push(ch);
            }
            prev_space = true;
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out
}
```

### 5.3 `clean_log`

```rust
pub fn clean_log(raw: &str) -> String {
    raw.lines()
        .map(strip_log_line)
        .collect::<Vec<_>>()
        .join("\n")
}
```

`raw.lines()` handles `\n`, `\r\n`, and trailing-newline edge cases uniformly. The output always uses `\n` as the line separator.

---

## 6. Error Handling

`strip_log_line` and `clean_log` are infallible. `strip-ansi-escapes::strip` returns `Vec<u8>` and does not fail; non-UTF-8 bytes are replaced via `from_utf8_lossy`. No `Result` type is needed.

---

## 7. Test Strategy

All tests live in `crates/aegis-tmux/src/log_clean.rs` under `#[cfg(test)]`. No tmux binary is required — all inputs are synthetic strings.

### 7.1 Unit tests

| Test name | Input | Expected output |
|---|---|---|
| `test_plain_text_unchanged` | `"hello world"` | `"hello world"` |
| `test_sgr_colour_stripped` | `"\x1b[31mred\x1b[0m"` | `"red"` |
| `test_bold_and_underline` | `"\x1b[1;4mbold\x1b[0m"` | `"bold"` |
| `test_cursor_up_stripped` | `"foo\x1b[Abar"` | `"foobar"` |
| `test_erase_line_stripped` | `"text\x1b[2K"` | `"text"` |
| `test_osc_title_stripped` | `"\x1b]0;my title\x07output"` | `"output"` |
| `test_osc_st_terminator` | `"\x1b]2;title\x1b\\"` | `""` |
| `test_carriage_return_stripped` | `"foo\rbar"` | `"foobar"` |
| `test_null_byte_stripped` | `"foo\x00bar"` | `"foobar"` |
| `test_trailing_spaces_trimmed` | `"hello   "` | `"hello"` |
| `test_interior_spaces_collapsed` | `"foo  bar  baz"` | `"foo bar baz"` |
| `test_blank_line_after_strip` | `"\x1b[2J\x1b[H"` | `""` |
| `test_clean_log_multiline` | `"line1\r\n\x1b[31mline2\x1b[0m\r\nline3"` | `"line1\nline2\nline3"` |
| `test_clean_log_preserves_blank_lines` | `"a\n\nb"` | `"a\n\nb"` |
| `test_real_claude_prompt` | A sample line with a full SGR sequence as captured from a real claude CLI session | Plain text content only |

### 7.2 Integration smoke test

In `LogTailer::tail` tests (in `aegis-controller`): verify that after the `strip_log_line` call-site change, lines streamed through the tailer contain no bytes in the range `\x1b` (`0x1B`). This catches any regression where the controller reverts to raw output.

### 7.3 Regression guard

Add a `#[test] fn no_ansi_in_output()` in `log_clean.rs` that generates a string containing every sequence category listed in §3.1, runs it through `clean_log`, and asserts the result contains no `\x1b` byte. This acts as a catch-all for any sequence the table-driven tests miss.
