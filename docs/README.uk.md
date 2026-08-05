# KernOX Monitor 0.2

KernOX Monitor — системний TUI на Rust. У Windows доступні системні показники,
процеси, накопичувачі, мережа та локальні Docker/Podman. У Linux додатково
визначаються init-система, GPU, hwmon, батареї, підсвічування, audio,
NetworkManager, BlueZ і SMART capabilities.

Збірка: `cargo build --release --locked`. Діагностика: `kernox-monitor doctor`.
Програма не зберігає пароль sudo. Зупинка процесів і контейнерів потребує окремого
підтвердження клавішею `y`.
