use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    Auto,
    English,
    Ukrainian,
    German,
    French,
    Spanish,
    Russian,
}

impl Language {
    pub fn resolve(self) -> Self {
        if self != Self::Auto {
            return self;
        }
        let locale = std::env::var("LANG")
            .or_else(|_| std::env::var("LC_ALL"))
            .unwrap_or_default()
            .to_lowercase();
        if locale.starts_with("uk") {
            Self::Ukrainian
        } else if locale.starts_with("de") {
            Self::German
        } else if locale.starts_with("fr") {
            Self::French
        } else if locale.starts_with("es") {
            Self::Spanish
        } else {
            Self::English
        }
    }

    pub fn next_visible(self) -> Self {
        match self.resolve() {
            Self::English => Self::Ukrainian,
            Self::Ukrainian => Self::German,
            Self::German => Self::French,
            Self::French => Self::Spanish,
            Self::Spanish | Self::Russian | Self::Auto => Self::English,
        }
    }
}

impl FromStr for Language {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "en" => Ok(Self::English),
            "uk" => Ok(Self::Ukrainian),
            "de" => Ok(Self::German),
            "fr" => Ok(Self::French),
            "es" => Ok(Self::Spanish),
            "ru" => Ok(Self::Russian),
            _ => Err(format!("unsupported language: {value}")),
        }
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Auto => "auto",
            Self::English => "en",
            Self::Ukrainian => "uk",
            Self::German => "de",
            Self::French => "fr",
            Self::Spanish => "es",
            Self::Russian => "ru",
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub enum TextKey {
    AppTitle,
    Overview,
    Processes,
    Storage,
    Containers,
    Network,
    Services,
    Hardware,
    Loading,
    Empty,
    Unavailable,
    PermissionDenied,
    Error,
    Stale,
    Help,
    Quit,
    Refresh,
    Search,
    Confirm,
    Cancel,
    TooSmall,
    Cpu,
    History,
    Memory,
    Swap,
    Uptime,
    Host,
    Kernel,
    Name,
    Status,
    Usage,
    Available,
    Mount,
    FileSystem,
    Received,
    Transmitted,
    Engine,
    Image,
    ServiceManager,
    Diagnostics,
    SecurityWarning,
    PressY,
    NoData,
}

pub fn tr(lang: Language, key: TextKey) -> &'static str {
    let l = lang.resolve();
    let values = match key {
        TextKey::AppTitle => [
            "KernOX Monitor",
            "KernOX Монітор",
            "KernOX Monitor",
            "KernOX Moniteur",
            "KernOX Monitor",
            "KernOX Монитор",
        ],
        TextKey::Overview => [
            "Overview",
            "Огляд",
            "Übersicht",
            "Vue d’ensemble",
            "Resumen",
            "Обзор",
        ],
        TextKey::Processes => [
            "Processes",
            "Процеси",
            "Prozesse",
            "Processus",
            "Procesos",
            "Процессы",
        ],
        TextKey::Storage => [
            "Storage",
            "Сховище",
            "Speicher",
            "Stockage",
            "Almacenamiento",
            "Накопители",
        ],
        TextKey::Containers => [
            "Containers",
            "Контейнери",
            "Container",
            "Conteneurs",
            "Contenedores",
            "Контейнеры",
        ],
        TextKey::Network => ["Network", "Мережа", "Netzwerk", "Réseau", "Red", "Сеть"],
        TextKey::Services => [
            "Services & Logs",
            "Служби й журнали",
            "Dienste & Protokolle",
            "Services et journaux",
            "Servicios y registros",
            "Службы и журналы",
        ],
        TextKey::Hardware => [
            "Hardware",
            "Обладнання",
            "Hardware",
            "Matériel",
            "Hardware",
            "Оборудование",
        ],
        TextKey::Loading => [
            "Loading",
            "Завантаження",
            "Laden",
            "Chargement",
            "Cargando",
            "Загрузка",
        ],
        TextKey::Empty | TextKey::NoData => [
            "No data",
            "Немає даних",
            "Keine Daten",
            "Aucune donnée",
            "Sin datos",
            "Нет данных",
        ],
        TextKey::Unavailable => [
            "Unavailable",
            "Недоступно",
            "Nicht verfügbar",
            "Indisponible",
            "No disponible",
            "Недоступно",
        ],
        TextKey::PermissionDenied => [
            "Permission denied",
            "Немає дозволу",
            "Zugriff verweigert",
            "Permission refusée",
            "Permiso denegado",
            "Доступ запрещён",
        ],
        TextKey::Error => ["Error", "Помилка", "Fehler", "Erreur", "Error", "Ошибка"],
        TextKey::Stale => [
            "Stale data",
            "Застарілі дані",
            "Veraltete Daten",
            "Données périmées",
            "Datos obsoletos",
            "Устаревшие данные",
        ],
        TextKey::Help => ["Help", "Довідка", "Hilfe", "Aide", "Ayuda", "Справка"],
        TextKey::Quit => ["Quit", "Вийти", "Beenden", "Quitter", "Salir", "Выйти"],
        TextKey::Refresh => [
            "Refresh",
            "Оновити",
            "Aktualisieren",
            "Actualiser",
            "Actualizar",
            "Обновить",
        ],
        TextKey::Search => ["Search", "Пошук", "Suche", "Recherche", "Buscar", "Поиск"],
        TextKey::Confirm => [
            "Confirm",
            "Підтвердити",
            "Bestätigen",
            "Confirmer",
            "Confirmar",
            "Подтвердить",
        ],
        TextKey::Cancel => [
            "Cancel",
            "Скасувати",
            "Abbrechen",
            "Annuler",
            "Cancelar",
            "Отмена",
        ],
        TextKey::TooSmall => [
            "Terminal is too small (minimum 80x24)",
            "Термінал замалий (мінімум 80x24)",
            "Terminal ist zu klein (mindestens 80x24)",
            "Terminal trop petit (minimum 80x24)",
            "Terminal demasiado pequeño (mínimo 80x24)",
            "Терминал слишком мал (минимум 80x24)",
        ],
        TextKey::Cpu => ["CPU", "CPU", "CPU", "CPU", "CPU", "CPU"],
        TextKey::History => [
            "History",
            "Історія",
            "Verlauf",
            "Historique",
            "Historial",
            "История",
        ],
        TextKey::Memory => [
            "Memory",
            "Пам’ять",
            "Speicher",
            "Mémoire",
            "Memoria",
            "Память",
        ],
        TextKey::Swap => ["Swap", "Swap", "Swap", "Swap", "Swap", "Swap"],
        TextKey::Uptime => [
            "Uptime",
            "Час роботи",
            "Laufzeit",
            "Disponibilité",
            "Actividad",
            "Аптайм",
        ],
        TextKey::Host => ["Host", "Вузол", "Host", "Hôte", "Host", "Хост"],
        TextKey::Kernel => ["Kernel", "Ядро", "Kernel", "Noyau", "Kernel", "Ядро"],
        TextKey::Name => ["Name", "Назва", "Name", "Nom", "Nombre", "Имя"],
        TextKey::Status => ["Status", "Стан", "Status", "État", "Estado", "Статус"],
        TextKey::Usage => [
            "Usage",
            "Використання",
            "Nutzung",
            "Utilisation",
            "Uso",
            "Использование",
        ],
        TextKey::Available => [
            "Available",
            "Доступно",
            "Verfügbar",
            "Disponible",
            "Disponible",
            "Доступно",
        ],
        TextKey::Mount => [
            "Mount",
            "Монтування",
            "Mount",
            "Montage",
            "Montaje",
            "Точка монтирования",
        ],
        TextKey::FileSystem => ["FS", "ФС", "FS", "FS", "FS", "ФС"],
        TextKey::Received => ["RX/s", "RX/с", "RX/s", "RX/s", "RX/s", "RX/с"],
        TextKey::Transmitted => ["TX/s", "TX/с", "TX/s", "TX/s", "TX/s", "TX/с"],
        TextKey::Engine => ["Engine", "Рушій", "Engine", "Moteur", "Motor", "Движок"],
        TextKey::Image => ["Image", "Образ", "Image", "Image", "Imagen", "Образ"],
        TextKey::ServiceManager => [
            "Service manager",
            "Менеджер служб",
            "Dienstemanager",
            "Gestionnaire de services",
            "Gestor de servicios",
            "Менеджер служб",
        ],
        TextKey::Diagnostics => [
            "Diagnostics",
            "Діагностика",
            "Diagnose",
            "Diagnostic",
            "Diagnóstico",
            "Диагностика",
        ],
        TextKey::SecurityWarning => [
            "Security warning",
            "Попередження безпеки",
            "Sicherheitswarnung",
            "Alerte de sécurité",
            "Aviso de seguridad",
            "Предупреждение безопасности",
        ],
        TextKey::PressY => [
            "Press y to confirm",
            "Натисніть y для підтвердження",
            "y zum Bestätigen",
            "Appuyez sur y",
            "Pulse y para confirmar",
            "Нажмите y для подтверждения",
        ],
    };
    values[match l {
        Language::English | Language::Auto => 0,
        Language::Ukrainian => 1,
        Language::German => 2,
        Language::French => 3,
        Language::Spanish => 4,
        Language::Russian => 5,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn visible_cycle_excludes_russian() {
        let mut lang = Language::English;
        for _ in 0..5 {
            lang = lang.next_visible();
            assert_ne!(lang, Language::Russian);
        }
        assert_eq!(lang, Language::English);
    }
    #[test]
    fn explicit_russian_is_supported() {
        assert_eq!("ru".parse(), Ok(Language::Russian));
    }
}
