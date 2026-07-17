//! Hand-rolled Ukrainian name/address data for the M4 demo tenant (PLAN.md:
//! "create and seed database 100 customers, 1000 orders with ukrainian
//! names"). The `fake` crate (v2.9) ships no `uk_UA` locale, so this is a
//! small curated pool instead of a `fake::locales` module — good enough for
//! demo data, not meant to be exhaustive.

use rand::Rng;
use rand::seq::SliceRandom;

const FIRST_NAMES: &[&str] = &[
    "Олександр",
    "Іван",
    "Петро",
    "Микола",
    "Андрій",
    "Тарас",
    "Богдан",
    "Роман",
    "Юрій",
    "Дмитро",
    "Олена",
    "Наталія",
    "Тетяна",
    "Оксана",
    "Ірина",
    "Марія",
    "Юлія",
    "Катерина",
    "Софія",
    "Вікторія",
];

const LAST_NAMES: &[&str] = &[
    "Шевченко",
    "Коваленко",
    "Бондаренко",
    "Ткаченко",
    "Кравченко",
    "Олійник",
    "Мельник",
    "Шевчук",
    "Поліщук",
    "Бойко",
    "Кузьменко",
    "Марченко",
    "Павленко",
    "Гончаренко",
    "Романюк",
];

const COMPANY_LEGAL_FORMS: &[&str] = &["ТОВ", "ПП", "ФОП"];

const COMPANY_NAMES: &[&str] = &[
    "Друкарня Либідь",
    "Поліграф-Сервіс",
    "Друкмастер",
    "Вернісаж Друк",
    "Графіка Плюс",
    "Колір Прінт",
    "Мідланд Друк",
    "Азбука Поліграфії",
    "Формат А1",
    "Прінт Хаус",
    "Літера Друк",
    "Каскад Поліграф",
];

const CITIES: &[&str] = &[
    "Київ",
    "Львів",
    "Одеса",
    "Харків",
    "Дніпро",
    "Запоріжжя",
    "Вінниця",
    "Полтава",
    "Чернігів",
    "Івано-Франківськ",
];

const STREETS: &[&str] = &[
    "вул. Шевченка",
    "вул. Франка",
    "вул. Хрещатик",
    "вул. Соборна",
    "вул. Грушевського",
    "вул. Незалежності",
    "вул. Лесі Українки",
    "вул. Січових Стрільців",
];

const EMAIL_DOMAINS: &[&str] = &["pryklad.ua", "poshta.ua", "druk.ua"];

pub const TAGS: &[&str] = &["поліграфія", "постійний", "опт", "новий", "vip"];

pub const CONTACT_ROLES: &[&str] = &["директор", "менеджер із закупівель", "бухгалтер"];

pub const PRODUCTS: &[&str] = &[
    "Візитки",
    "Листівки",
    "Брошури",
    "Плакати",
    "Банери",
    "Бланки",
    "Конверти",
    "Наклейки",
    "Поштові картки",
    "Каталоги",
];

pub const ORDER_NOTES: &[&str] = &[
    "Повторний друк: {product}",
    "Нове замовлення: {product}",
    "{product}, стандартний термін",
    "Терміново: {product}",
    "Щоквартальне поповнення: {product}",
    "{product} з погодженням макета",
    "{product} для нової кампанії",
];

pub fn company_name(rng: &mut impl Rng) -> String {
    let form = COMPANY_LEGAL_FORMS.choose(rng).unwrap();
    let name = COMPANY_NAMES.choose(rng).unwrap();
    format!("{form} «{name}»")
}

pub fn contact_name(rng: &mut impl Rng) -> String {
    let first = FIRST_NAMES.choose(rng).unwrap();
    let last = LAST_NAMES.choose(rng).unwrap();
    format!("{first} {last}")
}

pub fn email(rng: &mut impl Rng, seq: usize) -> String {
    let domain = EMAIL_DOMAINS.choose(rng).unwrap();
    format!("client{seq}@{domain}")
}

pub fn phone(rng: &mut impl Rng) -> String {
    format!("+380{:09}", rng.gen_range(500_000_000u32..699_999_999u32))
}

pub fn street(rng: &mut impl Rng) -> String {
    let name = STREETS.choose(rng).unwrap();
    let building = rng.gen_range(1..=120);
    format!("{name}, {building}")
}

pub fn zip(rng: &mut impl Rng) -> String {
    format!("{:05}", rng.gen_range(1000..99999))
}

pub fn city(rng: &mut impl Rng) -> String {
    (*CITIES.choose(rng).unwrap()).to_string()
}
