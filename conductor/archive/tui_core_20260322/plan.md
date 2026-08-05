# Implementation Plan: Tui Core

## Phase 1: Foundation & TUI Setup
- [x] Task: Initialize TUI Framework
    - [x] Настроить инициализацию терминала с использованием `crossterm` и `ratatui`.
    - [x] Реализовать основной асинхронный цикл приложения (`tokio`).
    - [x] Добавить базовую обработку клавиши `q` для выхода.
- [x] Task: UI Layout Architecture
    - [x] Определить структуру макета (верхний бар, боковая панель, основная область).
    - [x] Реализовать переключение между секциями (Tab/Numbers).
- [x] Task: Conductor - User Manual Verification 'Phase 1: Foundation & TUI Setup' (Protocol in workflow.md)

## Phase 2: System & Docker Integration
- [x] Task: System Monitor Display
    - [x] Интегрировать модуль `system.rs` для отображения загрузки CPU и RAM.
    - [x] Создать виджеты (графики/бары) для визуализации данных.
- [x] Task: Docker Integration UI
    - [x] Реализовать список контейнеров на основе данных из `docker.rs`.
    - [x] Отображать статус каждого контейнера.
- [x] Task: Conductor - User Manual Verification 'Phase 2: System & Docker Integration' (Protocol in workflow.md)

## Phase 3: System Modules & Polish
- [x] Task: Modules Interaction
    - [x] Добавить отображение и базовое управление для модулей `audio.rs`, `network.rs`, `brightness.rs`.
- [x] Task: Multilingual UI Support
    - [x] Реализовать поддержку русского и английского языков в интерфейсе.
- [x] Task: Conductor - User Manual Verification 'Phase 3: System Modules & Polish' (Protocol in workflow.md)
