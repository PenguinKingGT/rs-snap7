# Compatibility Notes

The wire baseline is recorded in [migration.md](migration.md). Rust interfaces
preserve supported representations without duplicating Python-specific layouts.

- `Area`, `WordLen`, `Parameter`, `BlockType`, and `ServerArea` retain upstream
  numeric identifiers; Rust names follow Rust naming conventions. Address
  constructors validate the entire 24-bit range and reject inconsistent areas.
- Scalar codecs use checked slices and explicit Rust numeric types. String
  lengths count encoded bytes. Latin-1 is the default; UTF-8 is explicitly
  supported. Arbitrary Python codec names are not accepted. WSTRING writes
  remain BMP-only. Invalid input is rejected before writes modify buffers.
- TIME/TO(D) and long duration functions return typed integer units, not formatted
  Python strings or timedelta. `Date` and `DateTime` are timezone-free calendar
  models. Long types retain nanoseconds that Python truncates to microseconds.
- DT writes truncate to milliseconds as upstream does. S5TIME is read-only.
  BCD, calendar, weekday ranges, and time-of-day fields are validated strictly;
  malformed values tolerated by permissive Python getters are rejected.

These differences are intentional; per-feature tests document valid wire values
and failure behavior. No PLC hardware has been validated yet.

- Transport validates COTP Class 0 headers and negotiation TLVs more strictly
  than the Python implementation. Fragmentation is bounded by the negotiated
  TPDU size; malformed frames and timeouts invalidate the connection.
