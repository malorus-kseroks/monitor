# Implementation Plan: Multi-language Expansion

## Phase 1: Core Localization Expansion
- [x] Task: Expand I18n Module
    - [x] Добавить новые варианты в перечисление `Language` в `src/i18n.rs`.
    - [x] Реализовать метод `next()` для циклического переключения языков.
    - [x] Изменить язык по умолчанию на English.
- [x] Task: Populate Translations
    - [x] Добавить переводы для украинского языка.
    - [x] Добавить переводы для немецкого языка.
    - [x] Добавить переводы для французского языка.
    - [x] Добавить переводы для испанского языка.
- [x] Task: Conductor - User Manual Verification 'Phase 1: Core Localization Expansion' (Protocol in workflow.md)

## Phase 2: CLI and Runtime Integration
- [x] Task: CLI Language Argument
    - [x] Обновить структуру `Args` в `src/main.rs`, добавив поле `lang`.
    - [x] Реализовать парсинг и применение языка при инициализации `App`.
- [x] Task: Runtime Cycle Switch
    - [x] Обновить обработчик клавиши `L` в `src/main.rs`, чтобы он использовал новый метод циклического переключения.
- [x] Task: Conductor - User Manual Verification 'Phase 2: CLI and Runtime Integration' (Protocol in workflow.md)

## Phase 3: Validation and Refinement
- [x] Task: UI Layout Check
    - [x] Проверить отображение длинных слов (особенно в немецком и французском языках) в заголовках и таблицах.
    - [x] При необходимости адаптировать ширину колонок.
- [x] Task: Conductor - User Manual Verification 'Phase 3: Validation and Refinement' (Protocol in workflow.md)
