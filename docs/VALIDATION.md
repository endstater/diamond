# Проверки Diamond 0.3.1

Дата: 21 ноября 2026. Пользователь подтвердил успешную сборку предыдущего
среза 0.1. Новая редакция 0.3.1 проверялась как исходный код; готового
исполняемого файла в этом комплекте нет.

## Выполнено

| Проверка | Результат |
| --- | --- |
| `python3 scripts/check_sources.py` | PASS: 6 workspace-пакетов, 7 TOML, локальные зависимости, отсутствие GUI-зависимостей в модели и vault; JSON, Python-синтаксис и SVG |
| `python3 scripts/check_plugin_example.py` | PASS: initialize, commands/list, Unicode-преобразование и ответ на неизвестный метод у Python-примера |
| `bash -n scripts/check.sh packaging/PKGBUILD` | PASS: синтаксис shell |
| Лексический просмотр Rust с проверкой парных разделителей | PASS для 19 исходных Rust-файлов; это не проверка типов или заимствований |
| Ручная сверка используемых egui/eframe API | Сверено с документацией версии 0.33.0 |
| Кодировка поставляемых исходников и примеров | UTF-8 читается без замены символов |

Проверка Python-примера не проверяет Rust-хост плагинов. Для Rust-хоста,
документа, редактора, файловой подсистемы и сессий добавлены отдельные
автоматические тесты, но здесь они не запускались.

Всего определено 79 Rust-тестов: 22 для документа/source map, 21 для
редактора/live/search/SVG, 11 для vault, 13 для сессий, фильтра событий и политики graphics swap,
9 для плагинов и 3 для общих настроек/regexp. Эти тесты здесь не запускались.


## Не выполнено

`scripts/check.sh` запускался и завершился с кодом 127 на первой команде:
`cargo: command not found`. В среде нет Cargo, rustc, rustfmt и Clippy.
Предыдущие попытки получить официальный toolchain и индекс crates.io получили HTTP 403.
Поэтому не выполнены:

- разрешение полного дерева зависимостей и создание Cargo.lock;
- компиляция workspace и release-бинарника;
- `cargo fmt --all -- --check`;
- `cargo test --workspace`;
- `cargo clippy --workspace --all-targets -- -D warnings`;
- проверка разрешённого дерева зависимостей на браузерные движки;
- запуск окна, IME, буфера обмена, Wayland/X11 и установка пакета Arch.

Исходный срез ещё нужно отформатировать и проверить компилятором. Исправления,
которые найдут компилятор, Clippy или тесты, нельзя считать заранее решёнными.

## На машине Arch Linux

Из корня распакованного проекта:

```sh
cargo fmt --all
./scripts/check.sh
cargo run --release -p diamond-app -- ./examples/vault
```

Первый запуск требует доступа к реестру зависимостей. Сохраните созданный
`Cargo.lock`, чтобы последующие сборки использовали те же версии. PKGBUILD
форматирует исходники в своей распакованной сборочной папке перед проверками.

Проверьте раскрытие активного Markdown-блока, ввод и удаление `#`, `**` и `*`,
положение курсора при раскрытии, непрерывное выделение и Unicode/IME,
undo/redo, переходы между режимами, запрет Live для других расширений,
автосохранение после 600 мс и перед закрытием, отмену IME при смене вкладки,
неизменность нетронутого Markdown и бинарных байтов,
конфликт после внешней записи, неверный hex и Save/Discard/Cancel. Для плагина
откройте `examples/plugins/uppercase/manifest.json`, подтвердите запуск,
явно разрешите передачу текста, проверьте результат перед применением и
попробуйте закрыть окно при работающем плагине.

Ограничения файловых гонок, модели Markdown и прав процессов плагинов
описаны в README соответствующих crates и в матрице возможностей.

## Приёмка 0.3 на рабочем столе

Проверьте полную высоту редактора на коротких и длинных файлах, темы Dark/Light/System, выбор акцента и шрифта, закрытие вкладки настроек, Ctrl+F/Ctrl+Shift+F с немедленным вводом, регистр/regexp/ошибочный regexp, подсветку и Esc, положение курсора при правках с открытым поиском, добавление/переименование вложенной папки извне, переключение хранилищ с грязными вкладками, отмену XDG-диалога и ручной fallback без портала. Положите собственный diamond2.svg в корень проекта и проверьте иконку после перезапуска. Реальные portal/inotify/theme/font/display сценарии здесь не запускались.

## Wayland 0.3.1

Пользователь сообщил о предупреждении при переключении с окна, восстановлении работы после возврата и отсутствии проблемы в XWayland. Главный поток в присланном стеке находился в Mesa EGL / wl_display_dispatch_queue / ppoll. По glutin 0.32.3 и исходникам eframe 0.33.0 сверено применение vsync к swap interval. Добавлены два теста автоматической политики и DIAMOND_VSYNC override; они не запускались. Изменение vsync не проверялось на живом Wayland/Hyprland, не измерялись частота кадров и загрузка CPU при анимациях. См. WAYLAND-0.3.1.md.

## 0.4.0 — 2026-11-21

Выполнены: `python3 scripts/check_sources.py` (6 crates; TOML, JSON, Python AST, SVG XML, локальные ассеты и архитектурные границы), `python3 scripts/check_plugin_example.py` (initialize, commands/list, Unicode invoke, unknown method), `bash -n scripts/check.sh packaging/PKGBUILD`. Отдельная проверка Pygments нашла корректный баланс Rust-разделителей и отсутствие лексических ошибок в 20 Rust-файлах; она не проверяет типы и заимствования. Содержимое архива сверено побайтово с исходниками, SVG сверён с предоставленным файлом.

В коде теперь 85 Rust-тестов. Добавлены сценарии ссылок/wikilinks в задачах с CRLF, переключения checkbox с undo/redo и сохранением адресов, проекции списков/ссылок/разделителей, границ удаления и отказа от перехода по симлинкам, удаления вкладок только после успешной операции, английского языка по умолчанию и Unicode-перевода. Rust-тесты не запускались.

`./scripts/check.sh` останавливается с `cargo: command not found` (код 127). Rustfmt, компиляция, Clippy, resolved dependency graph и реальное окно не проверены. Попытка установить tree-sitter для дополнительной синтаксической проверки не удалась; эта проверка также не заявляется. API FontData.index и Galley.cursor_from_pos сверены с исходниками egui 0.33.0; wikilink API/смещения — с pulldown-cmark 0.13.6.

На целевой машине: `cargo fmt --all`, `./scripts/check.sh`, release-сборка; затем проверить щелчок по круглым задачам и Ctrl+Z, `---` с раскрытием при курсоре, ссылки и aliases в списках, Ctrl+щелчок, одинаковую ширину и полный hit-area карточек поиска, свёрнутое дерево и фильтр с предками, подтверждение удаления с грязными вкладками и отказом по симлинкам, ширину вкладок при изменении окна, English/Russian/Chinese с установленным noto-fonts-cjk, отдельные шрифты интерфейса/Markdown и изменение их размеров. Обновление pulldown-cmark требует разрешить старому Cargo.lock измениться.

## 0.5.0 — 2026-11-22

Выполнены структурная проверка `scripts/check_sources.py`, smoke-тест Python-плагина, `bash -n scripts/check.sh packaging/PKGBUILD`, лексическая проверка Pygments для 24 Rust-файлов. Архив сверяется побайтово с исходниками при упаковке. Это не проверка Rust-типов, заимствований или разрешённых зависимостей.

В исходнике 95 Rust-тестов: добавлены последовательность допечатывания задач, незавершённые/разделённые пустой строкой списки со ссылками, точные Unicode/CRLF координаты таблиц, alt/title изображений, отсутствие reveal на пустом EOF, headless-кадры с потерянным фокусом окна и сохранённым фокусом TextEdit, грамматическая подсветка многострочных конструкций и кэш, отказ при некорректных изображениях, переименование без перезаписи/выхода из хранилища и сохранность грязного буфера при смене расширения. Они не запускались.

`bash scripts/check.sh` завершается кодом 127: `cargo: command not found`. Rustfmt, cargo build/test/clippy и native dependency graph не выполнены. API RawInput.focused, Galley coordinates, ShapeIdx/Painter.set и TextFormat.line_height сверены по egui 0.33.0; Syntect features и thread-safe regex cache — по 5.2.0; image limits/reader — по 0.25.8; ureq timeout API — по 2.12.1. Это сверка исходников, не замена компиляции.

На Arch: сохранить target/Cargo.lock, разрешить Cargo обновить lockfile для новых зависимостей, выполнить `cargo fmt --all`, `./scripts/check.sh`, release-сборку. Проверить все шаги присланной последовательности списков, Enter на EOF без следующего текста, ссылки с title/autolink/wiki в пунктах, таблицу с пустыми ячейками и выравниванием, открытый/закрытый fence с разными языками, локальное изображение и HTTP(S) загрузку/ошибку/повтор по клику, отсутствие сетевых загрузок при простом открытии заметки. Проверить размер и тему после отпускания ползунка, переименование грязной вкладки/коллизию имени/смену .md в .txt и нагрузку CPU при переходе в другое окно/рабочее пространство Hyprland. Утверждение об исправлении 70% CPU на реальной машине пока не делается.

### 0.5.0 build correction — E0689

The user's compiler reported E0689 at parse_inline's table-row saturating_sub call. The scalar position binding now explicitly uses usize, matching Mark/Link ranges and chars().count(). This is a source-only type annotation; package/dependency versions are unchanged for cache reuse. The user's log also launched a 0.4.0 executable after the failed build; use && before running target/release/diamond to avoid launching an old artifact on build failure. Structural checks pass; Rust compilation remains unavailable here.

## 0.6.0 — 2026-11-23

Базовые файлы GitHub сверены с 1a495d05518e439f7df73a3d7609ad6b6f2c98ce; логика lib.rs/live.rs 0.5 совпала, различалось форматирование. Новые тесты: уровни вложенных задач и смешанных списков, CRLF и переключение/undo задачи на глубине, многострочные и вложенные цитаты, Enter для маркеров/нумерации/задач/выхода из списка, LaTeX/sources/сноски и повторная нумерация, реальные размеры native math textures, строка заголовка/отступ кода, pointer press/release через настоящий native TextEdit. Всего в исходнике 102 Rust-теста. Они не запускались.

Структурная проверка исходников, smoke-тест Python-плагина, shell syntax и Rust lexical balance выполнены. Rust toolchain отсутствует, поэтому cargo build/test/clippy и rustfmt не выполнены. Патч проверяется через git apply --check --index к локальной копии точных базовых файлов, затем результат сверяется побайтово; это не проверка компиляции.

API latex-rust 2.1.1 (MathFont/STIX, SvgOptions, latex_to_svg), тэги math/footnote pulldown-cmark 0.13.6 и native egui 0.33.0 сверены по первичным исходникам. Optional egui backend математической библиотеки выключен; доступ к пользовательским шрифтам математическому парсеру не даётся.

На целевой машине после применения: cargo fmt --all, cargo build --release -p diamond-app (разрешить обновление Cargo.lock), cargo test --workspace, cargo clippy --workspace --all-targets -- -D warnings. Проверить pointer hover/press/release, checked tick и Ctrl+Z, Enter и Shift+Enter в Live/Source, 2–3 уровня смешанных списков/задач и цитат, пустые пункты, $inline$ / $$display$$ в абзаце/списке/таблице и ошибочную формулу, повторную сноску/Ctrl+click, рамку кода под предыдущим текстом и язык text/Rust/Python. Desktop/Hyprland и потребление CPU после этих изменений не измерялись.

## 0.7.0 — 2026-11-23

The user's previous 0.6 file updater was confirmed working by the user. This
iteration changes the source to 0.7.0; there is still no Rust toolchain in this
preparation environment. No cargo build/test/clippy, rustfmt, GUI run, native
package build, desktop rendering/performance or git-network installation is
claimed. Static checks cannot validate types, borrows or resolved dependency
versions. Downloading a Rust toolchain was unavailable here.

Executed checks: scripts/check_sources.py (workspace paths, TOML/JSON/Python/SVG,
crate GUI boundaries), Pygments Rust lexical tokens/delimiter balance, bash syntax
for both PKGBUILDs and check.sh, actual subprocess JSON-RPC tests of v1 and v2
Python examples. V2 checks verify PNG signature/dimensions, revision IDs,
Unicode scalar anchors and click-to-source navigation. The source-update archive
is byte/hash verified, and updater tests cover backup of local modifications,
check-only behavior, corrupt payload rejection, symlink refusal, preserved
.git/target/Cargo.lock, repeat installation and rollback on simulated write failure.

New Rust tests (not run) target file/folder no-replace moves, light file sessions,
nested task extraction, table Markdown preservation and Undo, deferred image HEX,
real extended grammar definitions, view frame bounds/NaN rejection and actual
v2 process startup/view rendering. The source count is recorded by the packaging
check rather than implying that any Rust test passed.

On Arch after updating, run cargo fmt --all, cargo build --release -p diamond-app,
cargo test --workspace, cargo clippy --workspace --all-targets -- -D warnings.
Allow Cargo.lock to update for Two-face/Syntect 5.3/Swash/rustix. For desktop
acceptance: test giant/corrupt PNG/JPEG/SVG and HEX switches, local/remote note
images, header gap/opacity on Hyprland at multiple scale factors, collapsed and
revealed wide tables (including context menus/Undo/escaped pipes), long math
inside paragraphs and lists, array shorthand, post-fence spacing, nested task
clicks/progress and reduced motion, full font list/Montserrat and color emoji with
Noto Color Emoji, Enter submission, no-replace folder rename/drag with dirty tabs,
CLI folder/file mode and makepkg -si -p PKGBUILD.local. Install the native-preview
example from its folder, launch explicitly, open a .previewdemo file, select the
view, edit/save and test source/preview wheel and click mapping. Test GitHub
installation on a network-enabled machine with a prepared public repository.

## 0.7.1 — 2026-11-23

Portal build correction: ashpd 0.11.0 OpenFileRequest::title accepts
Into<Option<&str>>, not an owned String. The worker now supplies
Some(title.as_str()); the owned title remains alive throughout the request.
The compiler-reported call is the only portal title setter in the app.
This targeted source correction has not been compiled here.

The user's Rust compiler exposed that `views` had been inserted into the
`Commands` response DTO instead of `PluginHost`. The field now belongs to the
host; removing it from the DTO also prevents v1 JSON responses from failing
with a missing-field error. Existing v1/v2 process integration tests cover this
protocol boundary when run with Cargo. SVG regression tests add real path
rasterization, XML declarations, BOM/long comments, a 10000×5000 vector viewport
scaled to 1200×600 and malformed XML rejection. These Rust tests are not run
here: Rust/Cargo remains unavailable. Do not interpret structural/lexical checks
as compiler, borrowing or native GUI validation.

License change: root LICENSE is the verbatim GNU GPL v3 text, Cargo metadata and
both Arch package recipes use GPL-3.0-only, and first-party source files carry
SPDX notices. Bundled third-party license files are unchanged. README is a short English overview with
the owner's SVG, features, source launch and Arch installation commands.

Preparation checks: TOML/JSON/Python/SVG structure, Rust lexical balance, shell
syntax, actual v1/v2 Python protocol fixtures and updater backup/rollback tests.
The update payload and archive bytes are compared against the prepared sources.
On the target machine run cargo fmt --all, cargo test --workspace, cargo clippy
--workspace --all-targets -- -D warnings and cargo run --release -p diamond-app.
Verify launching the old uppercase plugin and v2 preview example, opening SVG
in Image by default and switching to/from Hex without altering file bytes.

## 0.7.2 — 2026-11-23

Source-level performance changes are documented in PERFORMANCE.md. A newly
selected execution environment was awaited and checked; it also has no cargo or
rustc. No release build, Rust tests/clippy, native geometry verification, process
CPU figures or speedup claim are made here.

New Rust regressions cover shared map invalidation on edit/Undo/Redo, shared Undo
snapshots, history grouping without reparsing the source map, cached nested task
counts, blank-line/table height separation, quote content endpoints, retained
projection/galley on inactive redraw, more than 24 retained math textures, the
provided braced-array/relation shape, metadata cache reuse/atomic replacement and
six-digit HEX parsing. Existing source round-trip, filesystem conflict and v1/v2
plugin tests remain present. They are not run in this environment.

Preparation checks: source/TOML/JSON/Python/SVG structure, Rust lexical delimiter
balance, Python plugin subprocess fixtures, shell syntax and archive/update hash
and backup/rollback verification. These cannot substitute for Rust type/borrow
checking. The update includes the previous ashpd title type correction too.


## 0.7.3 source update

Added a 200 FPS ceiling in the native per-frame input hook, a steady caret,
static completion indicators, finite task-progress transitions, compensated
one-shot repaint deadlines and optional per-source repaint diagnostics.
The 0.7.2 vault index field mismatch is corrected: HeadingMetadata.text remains
String; NoteMetadata.text alone uses Arc<str>.

Source and upstream eframe/egui 0.33.0 APIs were inspected. Regression tests were
added for frame spacing and exact progress completion. Rust/Cargo is absent,
including in the newly selected environment: compilation, cargo fmt, workspace
tests, clippy, native GUI checks and CPU reduction remain unverified.
The root LICENSES directory was removed; Diamond uses GPL-3.0-only and bundled
third-party license notices remain intact.

Preparation verification for 0.7.3: all 14 updater tests passed, including
backup/rollback of removed notices, missing-file rollback, symlink rejection,
repeat updates and preservation of target/.git/Cargo.lock. Structural checks,
Rust token/delimiter checks and Arch recipe shell syntax passed. These checks
are not a substitute for compilation or runtime profiling.


## 0.7.4 source update

Added Rust regression cases for YAML round trips/CRLF/BOM, property edits and
Undo/Redo, malformed YAML rejection, complex-value retention, valid dates,
header reuse across body edits, tag exclusions and hierarchy, combined YAML/body
tag indexing, metadata projection/source anchors and retained source galleys.
These Rust tests are not run: Rust/Cargo remains unavailable. The new YAML
crate and eframe/egui APIs were checked against primary documentation/source,
but the dependency graph has not been resolved or compiled here.

Portable structure and Rust token/delimiter checks are run separately from native
validation. GUI appearance/interaction and CPU impact need verification on the
user's machine. The update retains the 200 FPS limit and previous compiler fixes.

Preparation results: all 15 updater tests passed, including 0.7.3 upgrades,
repeat installation, backup/rollback and cache/.git/lockfile preservation.
Source/TOML/JSON/Python/SVG structure, Rust lexical/delimiter checks and shell
syntax for the Arch recipes passed. These results do not establish Rust type
correctness or native behavior.


## 0.7.5 source update

Removed the separate property form and its source mutation/serialization API.
Added caret-aware source/render projection tests, closing-newline reveal-boundary
checks, malformed YAML fallback and source-edit comment/CRLF/BOM/Undo tests.
The source/frame/backlink caches and 200 FPS ceiling are retained.

Rust/Cargo is unavailable. Compilation, workspace tests, formatting, clippy,
real native keyboard/pointer interaction, rendering and CPU impact remain unrun.
Portable source/token checks and updater tests are separate checks.

Preparation checks for 0.7.5: 19 updater tests passed, including removal/backup
of the previous form, constructor-symlink rejection and restoration after a final
verification failure. Source structure, Rust token/delimiter checks and Arch
recipe shell syntax passed. The archive payload is byte-checked against sources.
These checks are not Rust compilation or native interaction verification.
