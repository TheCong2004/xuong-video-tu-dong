# Floword ↔ DonutBrowser local bridge

## What is implemented

- Shared render contract `1.0`, with a pinned SHA-256.
- Deterministic character-prompt compiler in the Floword frontend.
- An ArtCraft client that discovers DonutBrowser through a per-user manifest,
  checks the protocol SHA, and invokes the Donut helper with JSON files.
- A DonutBrowser helper sidecar named `floword-donut-bridge`.
- Durable receipts keyed by idempotency key. A repeated request returns the
  existing receipt; a concurrent claim returns `UNKNOWN`, not a duplicate
  provider submission.
- Donut startup registers its own bundled helper under
  `%LOCALAPPDATA%\\Floword\\DonutBridge\\donut-browser-install-v1.json`.

## What is deliberately not claimed yet

`grok.image.edit` provider submission is not implemented by this bridge yet.
`SUBMIT` therefore returns:

```text
status=REJECTED
submissionState=NOT_SUBMITTED
errorCode=GROK_ADAPTER_NOT_IMPLEMENTED
```

This is intentional. The bridge must not claim an image was rendered until the
Donut CDP automation has passed an authenticated-profile E2E test and returned
an artifact SHA-256.

## Required next E2E gate

1. Start an installed DonutBrowser build and confirm it writes the manifest.
2. Use the selected Donut profile to open a logged-in Grok image-edit page.
3. Implement the provider adapter inside DonutBrowser only: upload anchor,
   submit compiled prompt, wait for a result, copy artifact to the job folder,
   then return a `SUCCEEDED` receipt with path/hash/bytes.
4. ArtCraft validates that receipt before a scene can enter face-quality gate or
   CapCut assembly.

The ArtCraft video/OCR/translation queue is independent of this bridge; its
existing failure states must not be reinterpreted as bridge receipts.
