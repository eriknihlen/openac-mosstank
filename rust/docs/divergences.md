# Divergence register: where the Rust conversion deliberately differs

Every deliberate difference from the C# plugin's behaviour. A commit that
adds one adds its row; a commit that removes one deletes the row.

| Id | Where | C# | Rust | Why |
|---|---|---|---|---|
| COMPAT-1 | numbers (SDK kit) | some text uses the current culture | always the invariant culture | the kit's rule; MossTank's own C# formats invariantly almost everywhere |
| COMPAT-2 | numbers (SDK kit) | any format string | format strings the kit does not reproduce are errors | the kit's rule |
| COMPAT-3 | JSON (SDK kit) | indented JSON with the platform's line breaks | `\r\n` | the kit's rule |
| COMPAT-4 | JSON (SDK kit) | lenient readers | a strict reader | the kit's rule |
| COMPAT-5 | regex (SDK kit) | 100 ms match timeout | a backtracking step limit | deterministic on every machine; owner decision (plan question 8) |
| FMT-1 | `.af` reader | an `IF:` at the end of the file throws an index error; a pause or jump charge past the 32-bit range throws an overflow out of `TryLoadNav` | both are refusals (`expected a 'DO:' line.`, a shape error) | a reader never panics on malformed input |
| COMPAT-6 | invariant-culture order (SDK kit `text::collation`; `listsort` without an expression) | ICU's root collation for every character | .NET's order for U+0000–U+024F, General Punctuation and Currency Symbols; any other character sorts after those, by code point | the kit's rule: the full ICU table would add about a megabyte to the plugin |
| EXPR-2 | stopwatches, clocks | the machine's clocks, read directly | the host's clocks through `ExpressionClock` | a plugin reads time only through its host; a test gets a still clock |
| MON-1 | monster spec regex `#` (`mosstank-settings::monster::expression`) | .NET's `RegexParseException` and `RegexMatchTimeoutException` (25 ms) messages | the compatibility kit's messages; its step limit for the timeout | the kit has no .NET message text; the outcome (no match, the error reported) is the same |
| SET-1 | settings profile load and save (`mosstank-settings::profile`) | a row too short for a column it reads (the `Settings` table, the recharge plan), or a missing `Setting`/`Value` column met by `Save`, throws `ArgumentOutOfRangeException`, which the profile store does not catch | a `FormatError` carrying .NET's message, which the store (MT-3) will treat as a failed load | a reader never panics on malformed input |
| STORE-1 | UB settings files (`mosstank-stores::ub_store`) | each tier's file is read on its first use and written on every change | the tiers are read when the store is bound or loaded (the profile tier when a profile is chosen or made); a changed tier is written at `flush`, which the plugin calls before its call returns | the client is reached only during a plugin call, through the storage handed in, and the catalogue reads values without one; the files after a call are the same |
| STORE-2 | JSON a store reads (`mosstank-stores`) | System.Text.Json's `JsonException` text (with path, line and byte position) in the warning | the kit's parse error, or a conversion message naming the JSON path | the kit's reader has its own messages; the outcome (the file is reported and not written over) is the same |
| STORE-3 | paths shown to the player (`mosstank-stores::storage::display_path`) | `Path.Combine` with the platform's separator | the separator of the root the client describes (a backslash for a Windows root) | a WebAssembly plugin has no platform; on Windows the text is the same |
| STORE-4 | global variables' lock (`mosstank-stores::global_variables`) | a machine-wide named mutex, waited for | the client's storage lock, which refuses while another client holds it: that read or write fails as a storage failure | the plugin API's lock does not wait (a plugin call must not block the session); every hold is one file replace, so a refusal is rare and fails only the expression that met it |
