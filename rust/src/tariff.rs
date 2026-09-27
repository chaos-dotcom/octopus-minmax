//! Tariff model and the supported tariff list, mirroring `src/tariff.py`.

use std::sync::Mutex;

use fancy_regex::Regex;

pub struct Tariff {
    pub id: &'static str,
    pub display_name: &'static str,
    pub api_display_name: &'static str,
    pub tariff_code_matcher: &'static str,
    pub url_tariff_name: &'static str,
    pub switchable: bool,
    /// Set when the tariff is compared, exactly like the Python attribute.
    pub product_code: Mutex<Option<String>>,
}

impl Tariff {
    /// `re.search(self.tariff_code_matcher, current_tariff_name, re.IGNORECASE) is not None`
    pub fn is_tariff(&self, current_tariff_name: &str) -> bool {
        match Regex::new(&format!("(?i){}", self.tariff_code_matcher)) {
            Ok(pattern) => pattern.is_match(current_tariff_name).unwrap_or(false),
            Err(_) => false,
        }
    }

    /// The address Python's default object repr would print, so the dataclass repr
    /// in the log has the same shape (the address itself is not reproducible).
    pub fn python_address(&self) -> usize {
        self as *const Tariff as usize
    }

    pub fn get_product_code(&self) -> Option<String> {
        self.product_code.lock().unwrap().clone()
    }

    pub fn set_product_code(&self, value: Option<String>) {
        *self.product_code.lock().unwrap() = value;
    }
}

impl PartialEq for Tariff {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl std::fmt::Debug for Tariff {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "<tariff.Tariff object at 0x{:x}>", self.python_address())
    }
}

impl std::fmt::Display for Tariff {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Tariff(id={}, display_name={}, api_display_name={}, tariff_code_matcher={}, url_tariff_name={}, switchable={}, product_code={})",
            self.id,
            self.display_name,
            self.api_display_name,
            self.tariff_code_matcher,
            self.url_tariff_name,
            self.switchable,
            match self.get_product_code() {
                Some(code) => code,
                None => "None".to_string(),
            }
        )
    }
}

macro_rules! tariff {
    ($id:expr, $display:expr, $api:expr, $matcher:expr, $url:expr, $switchable:expr) => {
        Tariff {
            id: $id,
            display_name: $display,
            api_display_name: $api,
            tariff_code_matcher: $matcher,
            url_tariff_name: $url,
            switchable: $switchable,
            product_code: Mutex::new(None),
        }
    };
}

/// `tariff.TARIFFS` - same entries, same order.
pub fn all() -> Vec<&'static Tariff> {
    TARIFFS.iter().collect()
}

pub fn find(id: &str) -> Option<&'static Tariff> {
    TARIFFS.iter().find(|tariff| tariff.id == id)
}

static TARIFFS: [Tariff; 6] = [
    tariff!("go", "Octopus Go", "Octopus Go", r"-go-var-", "go", true),
    tariff!("go-fix-12m", "Octopus Go 12M Fixed", "Octopus Go 12M Fixed", r"-go-fix-", "go", true),
    tariff!("agile", "Agile Octopus", "Agile Octopus", r"-agile-", "agile", true),
    tariff!("cosy", "Cosy Octopus", "Cosy Octopus", r"-cosy-(?!.*fix)", "cosy-octopus", true),
    tariff!("cosy-fix", "Cosy Octopus 12M Fixed", "Cosy Octopus 12M Fixed", r"-cosy-.*fix", "cosy-octopus", false),
    tariff!("flexible", "Flexible Octopus", "Flexible Octopus", r"(?<!go-)var", "", false),
];
