# Cross-Backend Divergence Registry

Numbered, append-only. Each entry: behavior per backend, the OS limitation
forcing it, the test pinning it, and the review that accepted it. Rule
(RFC v2 §9): a divergence may cite only an OS limitation, never
implementation convenience.

_No entries yet. The first will almost certainly arrive with the Windows
Dir implementation (case-insensitive name collisions, sharing violations,
or delete-of-open-file semantics)._
