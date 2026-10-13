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
| FMT-2 | invariant casing (`mosstank-dotnet`) | .NET's (ICU) invariant case mapping | Rust's simple case mapping, one character to one character | the few characters where they differ (dotted capital I and the like) do not occur in profile files |
