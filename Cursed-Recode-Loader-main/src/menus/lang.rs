//! The client's own words in the menus, in the game's language (English or Russian; the others read English). The
//! game's own words (its options, its screens' titles) come translated from Java with the screens' data.

use std::sync::atomic::{AtomicU8, Ordering};

/// The client's own languages (the game's others read English; module names are English in all).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    En,
    Ru,
    Uk,
    Kk,
}

static LANG: AtomicU8 = AtomicU8::new(0);
static CODE: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
/// Bumped each time the language changes (the menus cross their words over to the new ones).
static VERSION: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// The game's language changed (its code, "ru_ru", "en_us"…).
pub fn set(code: &str) {
    let mut c = CODE.lock().unwrap();
    if *c != code {
        if !c.is_empty() {
            VERSION.fetch_add(1, Ordering::Relaxed);
        }
        *c = code.to_string();
    }
    let l = if code.starts_with("ru") || code.starts_with("be") {
        Lang::Ru
    } else if code.starts_with("uk") {
        Lang::Uk
    } else if code.starts_with("kk") {
        Lang::Kk
    } else {
        Lang::En
    };
    LANG.store(l as u8, Ordering::Relaxed);
}

/// The game's language code as it is now ("ru_ru").
pub fn code() -> String {
    CODE.lock().unwrap().clone()
}

pub fn version() -> u32 {
    VERSION.load(Ordering::Relaxed)
}

pub fn lang() -> Lang {
    match LANG.load(Ordering::Relaxed) {
        1 => Lang::Ru,
        2 => Lang::Uk,
        3 => Lang::Kk,
        _ => Lang::En,
    }
}

/// Russian: the menus' few words written out in Russian by hand (dates, plurals).
pub fn ru() -> bool {
    lang() == Lang::Ru
}

/// A word of the client's own by its key; the key itself when it has none.
pub fn tr(key: &str) -> &'static str {
    let l = lang();
    let more = match l {
        Lang::Uk => super::lang_more::UK,
        Lang::Kk => super::lang_more::KK,
        _ => &[],
    };
    if let Some((_, w)) = more.iter().find(|(k, _)| *k == key) {
        return w;
    }
    for (k, en, r) in WORDS {
        if *k == key {
            return if l == Lang::Ru { r } else { en };
        }
    }
    // a key with no word is a mistake in the menus: shown as it is, to be seen
    Box::leak(key.to_string().into_boxed_str())
}

/// Whether a key has its words (a word said by the game as it is — a sign-in's — is shown as it is otherwise).
pub fn known(key: &str) -> bool {
    WORDS.iter().any(|(k, _, _)| *k == key)
}

/// `tr(key)` with its %s filled in order.
pub fn trf(key: &str, args: &[&str]) -> String {
    let mut s = tr(key).to_string();
    for a in args {
        if let Some(i) = s.find("%s") {
            s.replace_range(i..i + 2, a);
        }
    }
    s
}

const WORDS: &[(&str, &str, &str)] = &[
    ("continue", "Continue", "Продолжить"),
    ("single", "Singleplayer", "Одиночная игра"),
    ("multi", "Multiplayer", "Сетевая игра"),
    ("settings", "Settings", "Настройки"),
    ("quit", "Quit", "Выход"),
    ("last", "Last server", "Последний сервер"),
    ("news", "What's new", "Что нового"),
    ("news.0", "Alpha 0.1", "Альфа 0.1"),
    ("news.1", "New launch and main menu", "Новый запуск и главное меню"),
    ("news.2", "Own Vulkan renderer", "Свой рендер на Vulkan"),
    ("news.3", "Discord status with your UID and nickname", "Статус в Discord с вашим UID и ником"),
    ("back", "Back", "Назад"),
    ("esc", "Esc", "Esc"),
    ("worlds", "%s worlds", "Миров: %s"),
    ("world1", "1 world", "1 мир"),
    ("ondisk", "%s on disk", "%s на диске"),
    ("newworld", "New world", "Новый мир"),
    ("play", "Play", "Играть"),
    ("played", "Played", "Наиграно"),
    ("size", "Size", "Размер"),
    ("version", "Version", "Версия"),
    ("lastplayed", "Last played", "Заходили"),
    ("cheats", "Commands", "Команды"),
    ("on", "On", "Вкл"),
    ("off", "Off", "Выкл"),
    ("backup", "Backup", "Копия"),
    ("folder", "Folder", "Папка"),
    ("rename", "Rename", "Имя"),
    ("edit", "Edit", "Изменить"),
    ("delete", "Delete", "Удалить"),
    ("hold", "Hold", "Удерживайте"),
    ("deleted", "%s deleted", "%s удалён"),
    ("undo", "Undo", "Вернуть"),
    ("create", "Create", "Создать"),
    ("cancel", "Cancel", "Отмена"),
    ("name", "Name", "Имя"),
    ("savedas", "Saved as %s", "Папка: %s"),
    ("random", "Random", "Случайно"),
    ("terrain", "The terrain you will get", "Каким будет рельеф"),
    ("servers", "%s servers", "Серверов: %s"),
    ("server1", "1 server", "1 сервер"),
    ("online", "%s players online", "Играют: %s"),
    ("join", "Join", "Войти"),
    ("direct", "Direct", "По адресу"),
    ("add", "Add", "Добавить"),
    ("refresh", "Refresh", "Обновить"),
    ("remove", "Hold to remove", "Удерживайте — убрать"),
    ("noanswer", "No answer", "Нет ответа"),
    ("asking", "Asking…", "Спрашиваю…"),
    ("vauto", "Auto", "Авто"),
    ("vsoon", "Before 1.20.2: soon", "До 1.20.2 — скоро"),
    ("address", "Address", "Адрес"),
    ("lasthere", "Last played here", "Играли здесь последним"),
    ("connecting", "Connecting to %s", "Подключение к %s"),
    ("loading", "Loading %s", "Загрузка: %s"),
    ("search", "Search", "Поиск"),
    ("reset", "Reset", "Сбросить"),
    ("press", "Press a key", "Нажмите клавишу"),
    ("clash", "Also %s", "Ещё: %s"),
    ("found", "%s found", "Найдено: %s"),
    ("drag", "Drag to turn", "Потяните, чтобы повернуть"),
    ("joinhint", "Double-click a server, or Enter", "Двойной клик по серверу или Enter"),
    ("draghint", "Drag to reorder", "Тяните, чтобы переставить"),
    ("today", "Today", "Сегодня"),
    ("yesterday", "Yesterday", "Вчера"),
    ("hours", "%s h", "%s ч"),
    ("minutes", "%s min", "%s мин"),
    ("seconds", "%s s", "%s с"),
    ("ms", "%s ms", "%s мс"),
    ("lan", "Looking for games on your network", "Ищу игры в локальной сети"),
    ("nothing", "Nothing here yet", "Здесь пока пусто"),
    ("done", "Done", "Готово"),
    ("yes", "Yes", "Да"),
    ("no", "No", "Нет"),
    ("ok", "OK", "OK"),
    ("game", "Game", "Игра"),
    ("resume", "Back to game", "Вернуться в игру"),
    ("lock", "Lock", "Закрепить"),
    ("resetkeys", "Reset keys", "Сбросить клавиши"),
    ("pause.world", "Singleplayer world", "Одиночный мир"),
    ("backedup", "Backed up", "Копия сделана"),
    ("flatnote", "No water, no hills: a plain to build on", "Ни воды, ни холмов: равнина для стройки"),
    ("remove1", "Remove", "Убрать"),
    ("adv", "Advancements", "Достижения"),
    ("stats", "Statistics", "Статистика"),
    ("obtained", "Obtained", "Получено"),
    ("notyet", "Not yet", "Ещё нет"),
    ("noadv", "Nothing here yet", "Здесь пока пусто"),
    ("advhint", "Drag to move · wheel to zoom · click a star", "Тяните, чтобы двигать · колесо — ближе · нажмите на звезду"),
    ("advback", "Click the sky to go back", "Нажмите на небо, чтобы вернуться"),
    ("remain", "Left", "Осталось"),
    ("critdone", "Done", "Сделано"),
    ("more", "more", "ещё"),
    ("nopic", "Its picture comes when you pause there", "Кадр появится, когда поставите там паузу"),
    ("pause.server", "On the server", "На сервере"),
    ("sec.video", "Video", "Графика"),
    ("sec.audio", "Audio", "Звук"),
    ("sec.controls", "Controls", "Управление"),
    ("sec.keys", "Key binds", "Клавиши"),
    ("sec.skin", "Skin", "Скин"),
    ("sec.chat", "Chat", "Чат"),
    ("sec.language", "Language", "Язык"),
    ("sec.packs", "Packs", "Наборы"),
    ("sec.access", "Accessibility", "Доступность"),
    ("sec.online", "Online", "Сеть"),
    ("locked", "Locked", "Закреплено"),
    ("killed", "Killed", "Убито"),
    ("travelled", "Travelled", "Пройдено"),
    ("gamedays", "%s of the world's days", "%s игровых дней"),
    ("mostkilled", "Killed most", "Чаще всего"),
    ("nemesis", "Most dangerous", "Опаснее всех"),
    ("killedyou", "Killed you", "Убил вас"),
    ("g.move", "Moving", "Передвижение"),
    ("g.fight", "Fighting", "Бой"),
    ("g.time", "Time", "Время"),
    ("g.world", "The world", "Мир"),
    ("g.other", "The rest", "Прочее"),
    ("zeros", "%s more at zero", "ещё %s — по нулям"),
    ("all", "All", "Всё"),
    ("killedby", "Killed you", "Убили вас"),
    ("creditshint", "Space — faster", "Пробел — быстрее"),
    ("nowplaying", "Now playing", "Сейчас играет"),
    ("saving", "Saving world", "Сохранение мира"),
    ("task", "Advancement", "Достижение"),
    ("goal", "Goal", "Цель"),
    ("challenge", "Challenge", "Испытание"),
    ("resetsection", "Reset the section", "Сбросить раздел"),
    ("sure", "Sure? Press again", "Точно? Нажмите ещё раз"),
    ("nothingfound", "Nothing found", "Ничего не нашлось"),
    // Recode's license (license.rs): what stops the game, what it shows while the time runs out
    ("lic.cap.start", "Launch", "Запуск"),
    ("lic.cap.sub", "Subscription", "Подписка"),
    ("lic.cap.account", "Account", "Аккаунт"),
    ("lic.cap.pc", "Computer", "Компьютер"),
    ("lic.cap.update", "Update", "Обновление"),
    ("lic.cap.net", "Connection", "Связь"),
    ("lic.loader", "Start it from the loader", "Запусти через лоудер"),
    ("lic.loader.text", "Recode opens from the Cursed Recode loader.", "Recode открывается из лоудера Cursed Recode."),
    ("lic.nosub", "No subscription", "Подписки нет"),
    ("lic.nosub.text", "Activate a key in your account on cursedclient.xyz.", "Активируй ключ в личном кабинете на cursedclient.xyz."),
    ("lic.ended", "The subscription has ended", "Подписка закончилась"),
    ("lic.ended.text", "Extend it with a key in your account on cursedclient.xyz.", "Продли её ключом в личном кабинете на cursedclient.xyz."),
    ("lic.ingame", "The account is already playing", "Аккаунт уже в игре"),
    ("lic.ingame.text", "Recode is running on this account on another computer.", "На этом аккаунте Recode уже запущен на другом компьютере."),
    ("lic.device", "Another computer", "Другой компьютер"),
    ("lic.device.text", "The subscription is tied to another computer. The loader frees it once a day.", "Подписка привязана к другому компьютеру. Сбросить привязку можно в лоудере раз в сутки."),
    ("lic.taken.text", "This computer is tied to another account.", "Этот компьютер привязан к другому аккаунту."),
    ("lic.banned", "Account banned", "Аккаунт заблокирован"),
    ("lic.reason", "Reason: %s.", "Причина: %s."),
    ("lic.banends", "The ban ends in %s.", "Бан закончится через %s."),
    ("lic.tpm", "TPM 2.0 is needed", "Нужен TPM 2.0"),
    ("lic.tpm.text", "Turn it on in the BIOS: Intel PTT or AMD fTPM.", "Включи его в BIOS: Intel PTT или AMD fTPM."),
    ("lic.outdated", "An update is out", "Вышло обновление"),
    ("lic.outdated.text", "Open the loader: it updates the game.", "Открой лоудер — он обновит игру."),
    ("lic.offline", "No connection to the server", "Нет связи с сервером"),
    ("lic.offline.text", "Check your internet and start again.", "Проверь интернет и запусти снова."),
    ("lic.offlinelong.text", "Over an hour without the server. Check your internet and start again.", "Больше часа без связи с сервером. Проверь интернет и запусти снова."),
    ("lic.session", "The session has ended", "Сессия закончилась"),
    ("lic.session.text", "Start Recode from the loader again.", "Запусти Recode через лоудер снова."),
    ("lic.openloader", "Open the loader", "Открыть лоудер"),
    ("lic.site", "Your account", "Личный кабинет"),
    ("lic.ending", "The subscription is ending", "Подписка заканчивается"),
    ("lic.closesin", "The game closes in", "Игра закроется через"),
    ("lic.extend", "Extend it with a key on cursedclient.xyz", "Продли ключом на cursedclient.xyz"),
    ("lic.backon", "Once it is back, the game goes on", "Вернётся связь — игра продолжится"),
    // ---- Alt Manager's card (alts.rs)
    ("alts.licence", "Licence", "Лицензия"),
    ("alts.new", "New nickname", "Новый ник"),
    ("alts.random", "Random", "Случайный"),
    ("alts.cursed", "From Cursed · %s", "Из Cursed · %s"),
    ("alts.next", "A new one counts from the next join", "Новый ник — со следующего входа"),
    ("alts.delete", "Delete?", "Удалить?"),
    ("3 to 16 letters", "3 to 16 letters", "От 3 до 16 символов"),
    ("Only Latin letters, digits and _", "Only Latin letters, digits and _", "Только латиница, цифры и _"),
    ("The list is full", "The list is full", "Список заполнен"),
    ("Opening the browser…", "Opening the browser…", "Открываю браузер…"),
    ("Sign in in the browser that opened", "Sign in in the browser that opened", "Войди в открывшемся браузере"),
    ("Adding the account", "Adding the account", "Добавляю аккаунт"),
    ("Signed in as", "Signed in as", "Вход выполнен:"),
    ("Renewing the licence of", "Renewing the licence of", "Обновляю лицензию:"),
    ("Licence ready", "Licence ready", "Лицензия готова:"),
    ("The sign-in failed", "The sign-in failed", "Вход не удался"),
    ("The licence could not be renewed", "The licence could not be renewed", "Не удалось обновить лицензию"),
    ("No answer from the browser: try again", "No answer from the browser: try again", "Браузер не ответил — попробуй ещё раз"),
    ("Something else answered: try again from the game", "Something else answered: try again from the game", "Ответил кто-то другой — попробуй ещё раз из игры"),
    ("The sign-in was cancelled", "The sign-in was cancelled", "Вход отменён"),
    ("Microsoft did not let in", "Microsoft did not let in", "Microsoft не пустил"),
    ("Microsoft did not give the tokens", "Microsoft did not give the tokens", "Microsoft не выдал ключи"),
    ("Microsoft asks to sign in again", "Microsoft asks to sign in again", "Microsoft просит войти заново"),
    ("The account's key cannot be opened on this Windows user: sign in again", "The account's key cannot be opened on this Windows user: sign in again", "Ключ аккаунта не открывается на этом пользователе Windows — войди заново"),
    ("Xbox Live did not let in", "Xbox Live did not let in", "Xbox Live не пустил"),
    ("This Microsoft account has no Xbox profile: make one at xbox.com and try again", "This Microsoft account has no Xbox profile: make one at xbox.com and try again", "У аккаунта Microsoft нет профиля Xbox — создай его на xbox.com и попробуй снова"),
    ("Xbox Live is not available in this account's country", "Xbox Live is not available in this account's country", "Xbox Live недоступен в стране аккаунта"),
    ("This account has to be verified as an adult's at xbox.com first", "This account has to be verified as an adult's at xbox.com first", "Сначала подтверди на xbox.com, что это аккаунт взрослого"),
    ("A child's account: an adult has to add it to a family at xbox.com first", "A child's account: an adult has to add it to a family at xbox.com first", "Детский аккаунт — взрослый должен добавить его в семью на xbox.com"),
    ("Minecraft's services do not let the client in yet: the app waits for Mojang's permission", "Minecraft's services do not let the client in yet: the app waits for Mojang's permission", "Сервисы Minecraft пока не пускают клиент — приложение ждёт разрешения Mojang"),
    ("Minecraft's services did not let in", "Minecraft's services did not let in", "Сервисы Minecraft не пустили"),
    ("This Microsoft account has no Minecraft: Java Edition", "This Microsoft account has no Minecraft: Java Edition", "На этом аккаунте Microsoft нет Minecraft: Java Edition"),
    ("The profile could not be read", "The profile could not be read", "Не удалось прочитать профиль"),
    ("The port for Microsoft's answer is taken by another program", "The port for Microsoft's answer is taken by another program", "Порт для ответа Microsoft занят другой программой"),
    ("Windows could not seal the account's key", "Windows could not seal the account's key", "Windows не смогла запечатать ключ аккаунта"),
    ("The profile's name cannot be played with", "The profile's name cannot be played with", "С этим именем профиля не войти на сервер"),
    ("Taken from Cursed", "Taken from Cursed", "Перенесено из Cursed:"),
    ("Signed in", "Signed in", "Вход выполнен"),
    ("Not signed in", "Not signed in", "Вход не выполнен"),
    ("Go back to the game: the account is being added.", "Go back to the game: the account is being added.", "Вернись в игру — аккаунт добавляется."),
    ("The sign-in was cancelled.", "The sign-in was cancelled.", "Вход отменён."),
    ("Something else answered: try again from the game.", "Something else answered: try again from the game.", "Ответил кто-то другой — попробуй ещё раз из игры."),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The license's words are in every language of the client (a missing one would quietly be English).
    #[test]
    fn the_licenses_words_are_in_every_language() {
        for (key, _, _) in WORDS.iter().filter(|w| w.0.starts_with("lic.")) {
            for (name, list) in [("uk", super::super::lang_more::UK), ("kk", super::super::lang_more::KK)] {
                assert!(list.iter().any(|(k, w)| k == key && !w.is_empty()), "{key} has no word in {name}");
            }
        }
    }
}
