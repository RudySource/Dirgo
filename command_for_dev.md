# Dirgo: команды разработчика

Внутренняя шпаргалка для локальной разработки, проверки обновлений и выпуска
релизов. Это не пользовательская инструкция.

## 1. Перейти в репозиторий и проверить состояние

```bash
cd /path/to/Dirgo
git status --short --branch
git diff --check
git diff
```

Не использовать `git add .` перед релизом. Добавлять только проверенные файлы,
чтобы случайно не опубликовать локальные данные, секреты или служебные материалы.

## 2. Локальная сборка

Debug-сборка:

```bash
cargo build --locked --bin dgo
./target/debug/dgo --version
```

Release-сборка:

```bash
cargo build --release --locked --bin dgo
./target/release/dgo --version
```

Установить текущий локальный код через Cargo:

```bash
cargo install --path . --locked --force
dgo --version
```

## 3. Форматирование, проверки и тесты

Быстрая проверка во время разработки:

```bash
cargo fmt
cargo check --all-targets --all-features
cargo test --locked
```

Строгая проверка перед коммитом:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
git diff --check
```

Проверить дополнительные release targets:

```bash
cargo check --locked --target x86_64-pc-windows-msvc --all-targets --all-features
cargo check --locked --target x86_64-unknown-linux-gnu --all-targets --all-features
cargo check --locked --target x86_64-apple-darwin --all-targets --all-features
```

Полный локальный release gate:

```bash
scripts/release-preflight.sh --require-fish
```

Успешное завершение должно содержать:

```text
PREFLIGHT:local-gates:ok
```

Отдельные терминальные проверки:

```bash
export DGO_BIN="$PWD/target/release/dgo"
expect scripts/pty-picker-smoke.exp
expect scripts/pty-terminal-gates.exp
expect scripts/pty-shell-matrix.exp
```

## 4. Проверка `dgo --update`

Команда определяет источник активного бинарника и использует соответствующий
способ обновления:

- Homebrew: `brew upgrade rudysource/tap/dirgo`;
- Cargo: `cargo install dirgo --version <VERSION> --locked`;
- Scoop: `scoop update dirgo`;
- прямая установка: проверенный GitHub release installer.

Проверить команду на локальной сборке:

```bash
cargo build --bin dgo
./target/debug/dgo --update
```

Если локальная версия новее опубликованной, установка не запускается и команда
печатает, что Dirgo уже обновлён.

## 5. Проверка уведомлений об обновлении

Отключить уведомления:

```bash
dgo update-notifications off
```

Включить обратно:

```bash
dgo update-notifications on
```

Полностью пропустить проверку только для одного процесса или тестового запуска:

```bash
DGO_DISABLE_UPDATE_CHECK=1 dgo <query>
```

Файлы механизма обновлений по XDG:

```text
${XDG_CACHE_HOME:-~/.cache}/dirgo/update.json
${XDG_CACHE_HOME:-~/.cache}/dirgo/update-check
${XDG_STATE_HOME:-~/.local/state}/dirgo/update-notifications-disabled
```

Проверка GitHub выполняется отдельным фоновым процессом не чаще одного раза в
сутки. Обычная навигация читает только локальный кэш и не ждёт сеть.

## 6. Подготовить новую версию

Пример ниже использует `0.3.1`. Для следующего релиза заменить номер во всех
командах на новый и не переиспользовать уже опубликованную версию.

1. Обновить `version` в `Cargo.toml`.
2. Обновить `Cargo.lock` обычной проверкой Cargo.
3. Перенести готовые изменения из `Unreleased` в датированный раздел
   `CHANGELOG.md`.
4. Проверить, что бинарник сообщает ожидаемую версию.

```bash
cargo check
cargo build --release --locked --bin dgo
./target/release/dgo --version
```

Проверить, что тег ещё не существует локально и удалённо:

```bash
git tag --list v0.3.1
git ls-remote --tags origin refs/tags/v0.3.1
```

Обе команды не должны выводить существующий тег.

## 7. Одноразовая настройка публикации

Авторизация GitHub CLI:

```bash
gh auth login -h github.com
gh auth status
```

Авторизация crates.io без сохранения токена в истории shell:

```bash
cargo login
```

Для автоматического обновления отдельного Homebrew tap создать fine-grained
GitHub token с доступом Contents read/write только к
`RudySource/homebrew-tap`, затем сохранить его в secrets репозитория Dirgo:

```bash
gh secret set HOMEBREW_TAP_TOKEN --repo RudySource/Dirgo
gh secret list --repo RudySource/Dirgo
```

## 8. Commit релиза 0.3.1

Сначала выполнить полный preflight:

```bash
scripts/release-preflight.sh --require-fish
```

Добавить только файлы этого релиза:

```bash
git add Cargo.toml Cargo.lock CHANGELOG.md CONTRIBUTING.md README.md \
  command_for_dev.md scripts/pty-picker-smoke.exp src/app.rs src/cli.rs \
  src/index.rs src/lib.rs src/paths.rs src/shell.rs src/tui.rs src/update.rs \
  tests/cli.rs
```

Проверить staged diff:

```bash
git diff --cached --check
git diff --cached --stat
git diff --cached
```

Создать commit:

```bash
git commit -m "Release Dirgo 0.3.1"
```

Проверить crate из чистого commit:

```bash
cargo publish --dry-run --locked
cargo package --locked --list
```

## 9. Push и GitHub Release

Сначала отправить commit:

```bash
git push origin main
```

Создать и отправить аннотированный тег, который точно совпадает с версией из
`Cargo.toml`:

```bash
git tag -a v0.3.1 -m "Dirgo v0.3.1"
git push origin v0.3.1
```

Push тега запускает `.github/workflows/release.yml`. Workflow:

1. тестирует и собирает четыре платформы;
2. создаёт архивы, installers и `SHA256SUMS`;
3. публикует GitHub Release и attestations;
4. обновляет `RudySource/homebrew-tap`, если настроен
   `HOMEBREW_TAP_TOKEN`.

Найти и дождаться workflow:

```bash
RUN_ID="$(gh run list \
  --repo RudySource/Dirgo \
  --workflow release.yml \
  --branch v0.3.1 \
  --limit 1 \
  --json databaseId \
  --jq '.[0].databaseId')"

gh run watch "$RUN_ID" --repo RudySource/Dirgo --exit-status
gh release view v0.3.1 --repo RudySource/Dirgo
```

Не публиковать crates.io, пока GitHub Release workflow не завершился успешно.

## 10. Опубликовать crates.io

Финальная непубликующая проверка:

```bash
cargo publish --dry-run --locked
```

Публикация:

```bash
cargo publish --locked
```

Проверка опубликованной версии:

```bash
cargo info dirgo@0.3.1
cargo install dirgo --version 0.3.1 --locked
dgo --version
```

Версию crates.io нельзя перезаписать или удалить. Если обнаружена ошибка,
выпустить следующий patch-релиз. `cargo yank` использовать только для запрета
новых установок сломанной версии, но не как способ её заменить.

## 11. Homebrew

Homebrew formula обновляется release workflow автоматически после успешного
GitHub Release.

Проверить workflow и формулу:

```bash
gh run list --repo RudySource/Dirgo --workflow release.yml --limit 3
brew update
brew info rudysource/tap/dirgo
```

Проверить обновление установленного Dirgo:

```bash
brew upgrade rudysource/tap/dirgo
dgo --version
```

Если Homebrew job был пропущен, проверить наличие секрета:

```bash
gh secret list --repo RudySource/Dirgo
```

## 12. Scoop

`RudySource/scoop-bucket` использует `checkver` и `autoupdate`. Excavator
проверяет новые GitHub Releases каждые четыре часа.

Запустить обновление немедленно:

```bash
gh workflow run excavator.yml --repo RudySource/scoop-bucket

SCOOP_RUN_ID="$(gh run list \
  --repo RudySource/scoop-bucket \
  --workflow excavator.yml \
  --limit 1 \
  --json databaseId \
  --jq '.[0].databaseId')"

gh run watch "$SCOOP_RUN_ID" \
  --repo RudySource/scoop-bucket \
  --exit-status
```

Проверка на Windows:

```powershell
scoop update
scoop update dirgo
dgo --version
```

## 13. Финальная проверка опубликованного релиза

```bash
gh release view v0.3.1 --repo RudySource/Dirgo
cargo info dirgo@0.3.1
brew info rudysource/tap/dirgo
```

Проверить release installer на чистом временном окружении или машине:

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/RudySource/Dirgo/releases/latest/download/dirgo-installer.sh | sh

dgo --version
dgo doctor
```

## 14. Если релиз завершился ошибкой

- Не перемещать и не перезаписывать уже опубликованный тег.
- Не выполнять force-push опубликованного release-тега.
- Исправить причину отдельным commit.
- Если версия уже появилась на crates.io или GitHub Releases, увеличить patch
  version и выпустить новый релиз.
- Если тег отправлен, но GitHub Release ещё не создан, сначала изучить упавший
  workflow:

```bash
gh run list --repo RudySource/Dirgo --workflow release.yml --limit 5
gh run view <RUN_ID> --repo RudySource/Dirgo --log-failed
```

После исправления повторно прогнать:

```bash
scripts/release-preflight.sh --require-fish
cargo publish --dry-run --locked
```

## 15. Короткий release checklist

```text
[ ] Cargo.toml и Cargo.lock содержат новую версию
[ ] CHANGELOG.md содержит датированный раздел
[ ] scripts/release-preflight.sh --require-fish прошёл
[ ] staged diff проверен вручную
[ ] cargo publish --dry-run --locked прошёл
[ ] main отправлен
[ ] аннотированный тег отправлен
[ ] GitHub Release workflow завершился успешно
[ ] Homebrew formula обновилась
[ ] cargo publish --locked выполнен
[ ] Scoop Excavator завершился
[ ] публичные способы установки показывают новую версию
```
