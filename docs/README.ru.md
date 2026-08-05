# KernOX Monitor 0.2

KernOX Monitor — системный TUI на Rust. В Windows доступны системные показатели,
процессы, накопители, сеть и локальные Docker/Podman. В Linux дополнительно
определяются init-система, GPU, hwmon, батареи, подсветка, audio, NetworkManager,
BlueZ и SMART capabilities.

Сборка: `cargo build --release --locked`. Диагностика: `kernox-monitor doctor`.
Программа не хранит пароль sudo. Операции остановки процессов и контейнеров
требуют отдельного подтверждения клавишей `y`.

Публичная история пока содержит старый пароль и не готова к релизу. Локальная
разработка не выполняет push или force-push.
