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
