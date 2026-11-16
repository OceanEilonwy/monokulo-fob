# Fob for Apple: coordination

The teams (core, iphone, ipad, macos, ci) talk only through one SQLite
database shared by every worktree. It is never committed. This file belongs
to core; see `plan.md` for the architecture and folder ownership.

```sh
cd /home/henry/Downloads/monokulo-fob   # or any worktree under it
DB="$(git rev-parse --path-format=absolute --git-common-dir)/fob-apple/fob.db"
sqlite3 "$DB"
```

Rules:

- Every write is inside `BEGIN IMMEDIATE; ... COMMIT;` (WAL mode is on, so
  readers never block).
- Claim a task only with the guarded update below and check `changes() = 1`;
  if it is 0, someone else got it: pick another.
- Edit only your own folders. Need something elsewhere? Insert a
  `core_requests` row (for core) or a `messages` row (for anyone else).
- A task's `id` is also its screenshot name: `<task_id>--<theme>` (plan.md §5).

## Schema

```sql
packs(id TEXT PRIMARY KEY, team TEXT, title TEXT, summary TEXT, status TEXT DEFAULT 'open')

tasks(id TEXT PRIMARY KEY, pack_id TEXT REFERENCES packs(id), team TEXT,
      kind TEXT CHECK(kind IN ('infra','component','screen','behaviour','test')),
      title TEXT,
      spec TEXT,          -- concrete acceptance: what is on screen / what happens, scene + theme, board
      design_board TEXT,  -- docs/apple/design/<board>.dc.html (comma-separated)
      scene TEXT,         -- -FobScene name(s); '*' all, '-' none
      paths TEXT,         -- the folders the task may touch
      status TEXT DEFAULT 'todo' CHECK(status IN
        ('todo','in_progress','needs_ci','ci_queued','ci_failed','review','done','blocked')),
      claimed_by TEXT, attempts INTEGER DEFAULT 0, updated_at TEXT DEFAULT CURRENT_TIMESTAMP)

task_deps(task_id TEXT REFERENCES tasks(id), depends_on TEXT REFERENCES tasks(id),
          PRIMARY KEY(task_id, depends_on))

core_requests(id INTEGER PRIMARY KEY, from_team TEXT, task_id TEXT, need TEXT,
              status TEXT DEFAULT 'open' CHECK(status IN ('open','accepted','done','rejected')),
              core_task_id TEXT, response TEXT,
              created_at TEXT DEFAULT CURRENT_TIMESTAMP, updated_at TEXT)

ci_requests(id INTEGER PRIMARY KEY, task_id TEXT, team TEXT,
            platform TEXT CHECK(platform IN ('iphone','ipad','macos','check')),
            branch TEXT, commit_sha TEXT,
            expect TEXT,      -- JSON list of screenshot names / test names
            status TEXT DEFAULT 'queued' CHECK(status IN ('queued','batched','passed','failed','superseded')),
            run_id INTEGER,   -- ci_runs.id
            created_at TEXT DEFAULT CURRENT_TIMESTAMP)

ci_runs(id INTEGER PRIMARY KEY, gh_run_id TEXT, integration_sha TEXT,
        branches TEXT,        -- JSON {"apple/iphone": "<sha>", ...}
        status TEXT,          -- running | passed | failed | partial
        summary TEXT, started_at TEXT, finished_at TEXT)

ci_findings(id INTEGER PRIMARY KEY, run_id INTEGER, platform TEXT, team TEXT, task_id TEXT,
            kind TEXT CHECK(kind IN ('screenshot','error','warning','info','merge_conflict','test_failure')),
            title TEXT, detail TEXT,
            artifact_path TEXT,   -- local path of the downloaded PNG / log
            read INTEGER DEFAULT 0, created_at TEXT DEFAULT CURRENT_TIMESTAMP)

messages(id INTEGER PRIMARY KEY, from_team TEXT, to_team TEXT, task_id TEXT, body TEXT,
         read INTEGER DEFAULT 0, created_at TEXT DEFAULT CURRENT_TIMESTAMP)
```

Extras: indexes on `tasks(team,status)`, `ci_requests(status)`,
`ci_findings(team,read)`, `messages(to_team,read)`; a trigger that bumps
`tasks.updated_at` when `status`, `claimed_by` or `attempts` change; and a
view **`ready_tasks`**: `todo` tasks whose dependencies are all `done`.

To recreate an empty database, run the `CREATE` statements above (with
`PRAGMA journal_mode=WAL;`) after `mkdir -p "$(dirname "$DB")"`.

## Task lifecycle

```
todo ─claim─> in_progress ─push + ci_request─> needs_ci ─CI batches─> ci_queued
                  ^                                                     │
                  │                                     ┌───────────────┴──────────────┐
                  └──────────────── ci_failed <─failed──┘                       passed─> review ─team compares
                                                                                          screenshot to board─> done
blocked: waiting on a core_request or another team (say why in messages).
```

- `in_progress`: claimed by the owning team.
- `needs_ci`: the team pushed its branch and inserted a `ci_requests` row.
- `ci_queued`: the CI agent merged the branch into `apple/integration` and
  started a run (`ci_requests.status='batched'`, `run_id` set).
- `review`: CI passed; each screenshot is a `ci_findings` row
  (`kind='screenshot'`, `artifact_path` = the PNG). The owning team opens
  the PNG beside `docs/apple/design/<board>.dc.html` and either marks the
  task `done` or sends it back to `in_progress` with a note.
- `ci_failed`: findings explain why; the team fixes it and goes back to
  `in_progress` (`attempts` + 1).
- Tasks with no screenshot (Rust or unit tests only) still go through CI
  (`platform='check'` or the platform whose build proves them) and are
  marked `done` by the owner once it passes.

## Conventions

- **Task ids**: `CORE-0xx`, `IPH-0xx`, `IPAD-0xx`, `MAC-0xx`. Packs:
  `CORE-A` scenes and words, `CORE-B` AppModel and view models, `CORE-C`
  FobUI theme and components, `CORE-D` platform services, `IPH-A`,
  `IPAD-A`, `MAC-A`.
- **Scenes** (fob-scenes `NAMES`, CORE-001): `home`, `home-syncing`,
  `receive`, `send`, `send-form`, `confirm`, `confirm-joel`, `pay-who`,
  `payment`, `contacts`, `settings`, `welcome`, `backup`, `catchup`,
  `coins`, `split`; plus `gallery-<component>` for FobUI components. A
  scene opens its screen directly (`AppModel.route`, CORE-011).
- **Launch arguments**: `-FobScene <scene>`, `-FobTheme oled|dark|light`,
  `-FobBiometrics pass|fail|unavailable` (scene mode never prompts; default
  pass), `-FobScanned <code>` (camera stub result).
- **Screenshots**: attachment `<task_id>--<theme>`; extra shots in one
  test `<task_id>-<step>--<theme>`. Each test waits on the accessibility id
  named in its spec (e.g. `balance`, `receive-address`, `send-review`,
  `pay-biometric`).
- **Components** are proved by core in a gallery: FobUI `GalleryView`
  shown by `FobRoot` for `-FobScene gallery-<name>`, tested by the
  core-owned bundle `FobCoreUITests` (`apple/core/UITests`) that runs in the
  iphone CI job (set up in CORE-020).
- **Branches** for `ci_requests.branch`: `apple/core`, `apple/iphone`,
  `apple/ipad`, `apple/macos`, `apple/ci`. `commit_sha` is the pushed head.

## Every team: start of a shift

```sql
-- unread messages and findings for me
SELECT id, from_team, task_id, body FROM messages WHERE to_team='iphone' AND read=0;
SELECT id, kind, task_id, title, artifact_path FROM ci_findings WHERE team='iphone' AND read=0;
-- my work in flight
SELECT id, status, title FROM tasks WHERE team='iphone' AND status NOT IN ('todo','done') ORDER BY id;
-- what I can start now
SELECT id, kind, title, scene, design_board FROM ready_tasks WHERE team='iphone' ORDER BY id;
-- why something is not ready
SELECT d.depends_on, u.team, u.status FROM task_deps d JOIN tasks u ON u.id=d.depends_on
 WHERE d.task_id='IPH-002' AND u.status<>'done';
```

Claim, then finish:

```sql
BEGIN IMMEDIATE;
UPDATE tasks SET status='in_progress', claimed_by='iphone' WHERE id='IPH-002' AND status='todo';
SELECT changes();          -- must be 1
COMMIT;

-- after pushing apple/iphone at <sha>
BEGIN IMMEDIATE;
INSERT INTO ci_requests(task_id, team, platform, branch, commit_sha, expect)
VALUES ('IPH-002', 'iphone', 'iphone', 'apple/iphone', '<sha>', '["IPH-002--oled"]');
UPDATE tasks SET status='needs_ci' WHERE id='IPH-002';
COMMIT;

-- reviewing a passed screenshot
BEGIN IMMEDIATE;
UPDATE ci_findings SET read=1 WHERE team='iphone' AND task_id='IPH-002';
UPDATE tasks SET status='done' WHERE id='IPH-002' AND status='review';
-- or: UPDATE tasks SET status='in_progress', attempts=attempts+1 WHERE id='IPH-002';
COMMIT;

-- mark messages read
BEGIN IMMEDIATE; UPDATE messages SET read=1 WHERE to_team='iphone' AND read=0; COMMIT;
```

## iphone / ipad / macos: asking core

```sql
BEGIN IMMEDIATE;
INSERT INTO core_requests(from_team, task_id, need)
VALUES ('ipad', 'IPAD-005', 'SendModel needs a "picking recipient" state so the sheet can push Pay who?');
UPDATE tasks SET status='blocked' WHERE id='IPAD-005' AND claimed_by='ipad';
COMMIT;
-- answers come back in core_requests.response / status
SELECT id, status, core_task_id, response FROM core_requests WHERE from_team='ipad' ORDER BY id DESC;
```

Talking to another device team (e.g. ipad needs a hook in iPhone's root):

```sql
BEGIN IMMEDIATE;
INSERT INTO messages(from_team, to_team, task_id, body)
VALUES ('ipad', 'iphone', 'IPAD-001', 'Please expose RootSwitch as a public view the Pad folder can replace.');
COMMIT;
```

## core: handling requests

```sql
SELECT id, from_team, task_id, need FROM core_requests WHERE status='open' ORDER BY id;

BEGIN IMMEDIATE;
INSERT INTO tasks(id, pack_id, team, kind, title, spec, design_board, scene, paths)
VALUES ('CORE-050', 'CORE-B', 'core', 'infra', 'SendModel: picking recipient',
        '<concrete acceptance>', 'PadSendTo', 'pay-who', 'apple/core/Sources/FobCore/**');
INSERT INTO task_deps(task_id, depends_on) VALUES ('IPAD-005', 'CORE-050');
UPDATE core_requests SET status='accepted', core_task_id='CORE-050', response='Planned as CORE-050',
       updated_at=CURRENT_TIMESTAMP WHERE id=1;
COMMIT;
-- when CORE-050 is done
BEGIN IMMEDIATE;
UPDATE core_requests SET status='done', updated_at=CURRENT_TIMESTAMP WHERE core_task_id='CORE-050';
INSERT INTO messages(from_team, to_team, task_id, body) VALUES ('core','ipad','IPAD-005','CORE-050 merged into apple/main.');
COMMIT;
```

Core merges finished work into `apple/main`; every team merges
`origin/apple/main` at the start of a shift.

## ci: batching, running, reporting

```sql
-- what to build next: newest request per (branch, platform)
SELECT id, task_id, team, platform, branch, commit_sha, expect FROM ci_requests
 WHERE status='queued' ORDER BY branch, platform, id;

BEGIN IMMEDIATE;
-- older requests on the same branch+platform are superseded by a newer sha
UPDATE ci_requests SET status='superseded'
 WHERE status='queued' AND EXISTS (SELECT 1 FROM ci_requests n WHERE n.status='queued'
   AND n.branch=ci_requests.branch AND n.platform=ci_requests.platform AND n.id>ci_requests.id
   AND n.task_id=ci_requests.task_id);
INSERT INTO ci_runs(gh_run_id, integration_sha, branches, status, started_at)
VALUES (NULL, '<sha of apple/integration>', '{"apple/iphone":"<sha>"}', 'running', CURRENT_TIMESTAMP);
UPDATE ci_requests SET status='batched', run_id=last_insert_rowid() WHERE status='queued';
UPDATE tasks SET status='ci_queued'
 WHERE id IN (SELECT task_id FROM ci_requests WHERE status='batched' AND run_id=<run>);
COMMIT;
-- once the GitHub run id is known
BEGIN IMMEDIATE; UPDATE ci_runs SET gh_run_id='<id>' WHERE id=<run>; COMMIT;
```

A branch that does not merge cleanly into `apple/integration` is left out
of the batch and reported:

```sql
BEGIN IMMEDIATE;
INSERT INTO ci_findings(run_id, platform, team, task_id, kind, title, detail)
VALUES (<run>, 'iphone', 'iphone', 'IPH-002', 'merge_conflict', 'apple/iphone conflicts with apple/ipad',
        'apple/ios/FobApp.swift');
UPDATE ci_requests SET status='failed', run_id=<run> WHERE id=<req>;
UPDATE tasks SET status='ci_failed', attempts=attempts+1 WHERE id='IPH-002';
COMMIT;
```

After the run (download `shots-<platform>` and `xcresult-<platform>`):

```sql
BEGIN IMMEDIATE;
-- one row per screenshot in the request's expect list
INSERT INTO ci_findings(run_id, platform, team, task_id, kind, title, artifact_path)
VALUES (<run>, 'iphone', 'iphone', 'IPH-002', 'screenshot', 'IPH-002--oled', '<dir>/shots-iphone/IPH-002--oled.png');
-- failures, with the failing test and the first error lines
INSERT INTO ci_findings(run_id, platform, team, task_id, kind, title, detail)
VALUES (<run>, 'macos', 'macos', 'MAC-004', 'test_failure', 'testConfirm', '<message>');
UPDATE ci_requests SET status='passed' WHERE run_id=<run> AND task_id='IPH-002';
UPDATE tasks SET status='review' WHERE id='IPH-002' AND status='ci_queued';
UPDATE ci_requests SET status='failed' WHERE run_id=<run> AND task_id='MAC-004';
UPDATE tasks SET status='ci_failed', attempts=attempts+1 WHERE id='MAC-004' AND status='ci_queued';
UPDATE ci_runs SET status='partial', summary='iphone passed, macos 1 failure', finished_at=CURRENT_TIMESTAMP
 WHERE id=<run>;
COMMIT;
```

A request is `passed` only if its job succeeded and every name in `expect`
came back as a PNG; a missing screenshot is a `test_failure` finding.

## Planner / overview queries

```sql
SELECT team, status, count(*) FROM tasks GROUP BY team, status ORDER BY team, status;
SELECT p.id, p.title, sum(t.status='done') || '/' || count(*) AS done
  FROM packs p JOIN tasks t ON t.pack_id=p.id GROUP BY p.id;
SELECT id, team, status, attempts FROM tasks WHERE attempts >= 3 OR status='blocked';
```

## Seeded plan (2026-11-16)

82 tasks in 7 packs. Core first: `CORE-001` (scene names) unblocks scene
data (`CORE-002..005`), then `CORE-010` AppModel and `CORE-011` routes,
then `CORE-020` theme + gallery, which unblocks the components
(`CORE-021..030`) and the device shells (`IPH-001`, `MAC-001`; `IPAD-001`
follows `IPH-001`).

| pack | team | tasks |
|---|---|---|
| CORE-A Scenes and words | core | 5 |
| CORE-B AppModel and view models | core | 10 |
| CORE-C FobUI theme and components | core | 11 |
| CORE-D Platform services (biometrics, camera, NFC, plist) | core | 4 |
| IPH-A iPhone app | iphone | 21 |
| IPAD-A iPad app | ipad | 17 |
| MAC-A macOS app | macos | 14 |
