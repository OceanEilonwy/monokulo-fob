# Fob for Apple: status

Written by the reporter from the coordination DB after round 4 (2026-11-16). apple/main is at 67cbdc6.

## Tasks per pack

| Pack | Team | Title | done | review | ci_failed | blocked | todo | total |
|---|---|---|---|---|---|---|---|---|
| CORE-A | core | Scenes and words | 4 | 1 | 0 | 0 | 0 | 5 |
| CORE-B | core | AppModel and view models | 0 | 1 | 0 | 0 | 9 | 10 |
| CORE-C | core | FobUI theme and components | 0 | 0 | 0 | 0 | 11 | 11 |
| CORE-D | core | Platform services | 0 | 0 | 0 | 0 | 4 | 4 |
| IPH-A | iphone | iPhone app | 0 | 0 | 0 | 0 | 21 | 21 |
| IPAD-A | ipad | iPad app | 0 | 0 | 0 | 0 | 17 | 17 |
| MAC-A | macos | macOS app | 0 | 0 | 0 | 0 | 14 | 14 |
| **All** | | | **4** | **2** | **0** | **0** | **76** | **82** |

- Done: CORE-001 (scene names), CORE-002 (send/confirm/pay-who data), CORE-003 (settings, welcome, backup, catch-up data), CORE-004 (sync states, split, iPad extra row).
- In review (CI passed, core marks them done next shift): CORE-005 (board words from fob-words), CORE-010 (AppModel with a WalletSource).
- No task is in progress, ci_failed or blocked. ready_tasks is empty until CORE-005 and CORE-010 are marked done. Then CORE-011 to CORE-019 become ready.

## Critical path

Every device task waits on one chain of core tasks, one after the other:

CORE-010 (review) -> CORE-011 route model -> CORE-020 theme tokens and gallery -> CORE-030 tab bar -> IPH-001 iPhone shell and MAC-001 Mac window -> IPAD-001 size-class switch.

Each step needs a CI pass before the next can start, so iPhone, iPad and macOS have done no work in four rounds.

## Open core_requests

None.

## Last CI run per platform

All three runs built apple/integration from apple/main plus apple/core. Runs 1 to 3 all passed.

| Platform | Last run | Result | What it proved |
|---|---|---|---|
| check | [43844445331](https://github.com/OceanEilonwy/monokulo-fob/actions/runs/43844445331) | passed | cargo check/test/clippy -D warnings on fob-ffi and fob-scenes; `labels::tests::every_board_word_is_here` |
| macos | [43844445331](https://github.com/OceanEilonwy/monokulo-fob/actions/runs/43844445331) | passed | First Swift build of the new FobCore code; FobCoreTests: AppModelTests 4/4, LabelsTests 2/2 |
| iphone | [43844445331](https://github.com/OceanEilonwy/monokulo-fob/actions/runs/43844445331) | passed | Scaffold UI test only |
| ipad | [43844445331](https://github.com/OceanEilonwy/monokulo-fob/actions/runs/43844445331) | passed on re-run | Attempt 1 was cancelled: the simulator took 12.5 min to boot. Attempt 2 passed the scaffold UI test |

Earlier runs: [43832960712](https://github.com/OceanEilonwy/monokulo-fob/actions/runs/43832960712) (CORE-001) and [43839183539](https://github.com/OceanEilonwy/monokulo-fob/actions/runs/43839183539) (CORE-002 to 004). Job times on the last run: check 1.5 min, macos 5 min, iphone 19.5 min, ipad 17 to 24 min.

## Screenshots

None of the done tasks has a screenshot. They are Rust scene data, checked by the `check` job. The only screenshots so far come from the scaffold apps (T-SCAFFOLD), from run 43844445331, in the shared git dir:

- `.git/fob-apple/runs/43844445331/iphone/shots-iphone/T-SCAFFOLD-iphone--oled.png`
- `.git/fob-apple/runs/43844445331/ipad/shots-ipad/T-SCAFFOLD-ipad--oled.png`
- `.git/fob-apple/runs/43844445331/macos/shots-macos/T-SCAFFOLD-macos--oled.png`

## Notes and differences from the boards

- Amount field line: the code says "≈ 0.0564 XMR · 1 XMR = A$354.60" but the board says "≈ 0.0564 XMR · A$354.60" (CORE-002).
- Split: with A$137 ready, fob-screens' fill cannot produce the board's 2:3:4 mix. Auto is offered but not lit, and the left-over and fee come out close to the board's A$17.23 and A$0.03 but not exact (CORE-004).
- The check job runs tests only for `-p fob-ffi -p fob-scenes`, so fob-words tests run locally and not in CI.
