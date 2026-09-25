//! UI string catalogue for the GUI.
//!
//! Right-to-left UI languages (Arabic, Hebrew, Persian) are deliberately not
//! offered: egui has no bidirectional-text or Arabic shaping support, so a
//! translated interface would render reversed and unjoined. Those scripts
//! are still fully supported as Morse *alphabets*.

/// A UI language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    Es,
    Fr,
    De,
    It,
    Pt,
    Ru,
    Uk,
    El,
    Ja,
    Ko,
    Zh,
}

impl Lang {
    pub const ALL: [Lang; 12] = [
        Lang::En,
        Lang::Es,
        Lang::Fr,
        Lang::De,
        Lang::It,
        Lang::Pt,
        Lang::Ru,
        Lang::Uk,
        Lang::El,
        Lang::Ja,
        Lang::Ko,
        Lang::Zh,
    ];

    /// The language's own name, for the picker.
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Es => "Español",
            Lang::Fr => "Français",
            Lang::De => "Deutsch",
            Lang::It => "Italiano",
            Lang::Pt => "Português",
            Lang::Ru => "Русский",
            Lang::Uk => "Українська",
            Lang::El => "Ελληνικά",
            Lang::Ja => "日本語",
            Lang::Ko => "한국어",
            Lang::Zh => "简体中文",
        }
    }

    /// True if the language needs a CJK font that egui doesn't bundle.
    pub fn needs_cjk_font(self) -> bool {
        matches!(self, Lang::Ja | Lang::Ko | Lang::Zh)
    }

    /// Map a POSIX locale string (`de_DE.UTF-8`, `pt-BR`, `zh_CN`) to a
    /// supported language, if any.
    pub fn from_locale(locale: &str) -> Option<Lang> {
        let code = locale
            .split(['_', '-', '.', '@'])
            .next()?
            .to_ascii_lowercase();
        Lang::ALL.into_iter().find(|l| l.code() == code)
    }

    /// Pick the UI language from the standard locale variables, falling
    /// back to English. `LANGUAGE` may hold a colon-separated preference list.
    pub fn from_env() -> Lang {
        let vars = ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"];
        vars.iter()
            .filter_map(|v| std::env::var(v).ok())
            .flat_map(|v| v.split(':').map(str::to_owned).collect::<Vec<_>>())
            .find_map(|l| Lang::from_locale(&l))
            .unwrap_or(Lang::En)
    }

    fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Es => "es",
            Lang::Fr => "fr",
            Lang::De => "de",
            Lang::It => "it",
            Lang::Pt => "pt",
            Lang::Ru => "ru",
            Lang::Uk => "uk",
            Lang::El => "el",
            Lang::Ja => "ja",
            Lang::Ko => "ko",
            Lang::Zh => "zh",
        }
    }
}

/// A translatable UI message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Msg {
    AppTitle,
    TextToMorse,
    MorseToText,
    TextLabel,
    MorseLabel,
    AlphabetLabel,
    AutoDetect,
    Prosigns,
    Result,
    Copy,
    Copied,
    SpeedHeading,
    CharSpeed,
    Farnsworth,
    FarnsworthHelp,
    EffectiveSpeed,
    Transmit,
    Tip,
    Language,
    MissingFont,
    PsEndOfMessage,
    PsEndOfContact,
    PsNewParagraph,
    PsOverToYou,
    PsWait,
    PsStartCopying,
}

impl Msg {
    pub const ALL: [Msg; 26] = [
        Msg::AppTitle,
        Msg::TextToMorse,
        Msg::MorseToText,
        Msg::TextLabel,
        Msg::MorseLabel,
        Msg::AlphabetLabel,
        Msg::AutoDetect,
        Msg::Prosigns,
        Msg::Result,
        Msg::Copy,
        Msg::Copied,
        Msg::SpeedHeading,
        Msg::CharSpeed,
        Msg::Farnsworth,
        Msg::FarnsworthHelp,
        Msg::EffectiveSpeed,
        Msg::Transmit,
        Msg::Tip,
        Msg::Language,
        Msg::MissingFont,
        Msg::PsEndOfMessage,
        Msg::PsEndOfContact,
        Msg::PsNewParagraph,
        Msg::PsOverToYou,
        Msg::PsWait,
        Msg::PsStartCopying,
    ];
}

/// Translate `msg` into `lang`.
pub fn tr(lang: Lang, msg: Msg) -> &'static str {
    let row: [&'static str; 26] = match lang {
        Lang::En => [
            "Morse Code Translator",
            "Text → Morse",
            "Morse → Text",
            "Text:",
            "Morse (letters space-separated, / between words):",
            "Alphabet:",
            "Auto-detect",
            "Prosigns:",
            "Result:",
            "Copy",
            "Copied to clipboard.",
            "Transmission speed",
            "Character speed:",
            "Farnsworth timing",
            "Send characters at full speed but stretch the pauses between letters/words — the method recommended by the ARRL and CW Academy for learning Morse, since it avoids the \"counting dits and dahs\" habit that caps your speed later.",
            "Effective speed:",
            "Transmit",
            "Tip: try \"SOS\" first, or click a prosign like <AR> to add it to your text.",
            "Language:",
            "No system font found for this script; some characters may show as boxes.",
            "end of message",
            "end of contact",
            "new paragraph",
            "over to you, specifically",
            "wait",
            "start copying",
        ],
        Lang::Es => [
            "Traductor de código Morse",
            "Texto → Morse",
            "Morse → Texto",
            "Texto:",
            "Morse (letras separadas por espacios, / entre palabras):",
            "Alfabeto:",
            "Detección automática",
            "Prosignos:",
            "Resultado:",
            "Copiar",
            "Copiado al portapapeles.",
            "Velocidad de transmisión",
            "Velocidad de caracteres:",
            "Temporización Farnsworth",
            "Envía los caracteres a velocidad completa pero alarga las pausas entre letras y palabras: el método que recomiendan la ARRL y CW Academy para aprender Morse, porque evita el hábito de «contar puntos y rayas» que luego limita tu velocidad.",
            "Velocidad efectiva:",
            "Transmitir",
            "Consejo: prueba primero «SOS» o pulsa un prosigno como <AR> para añadirlo al texto.",
            "Idioma:",
            "No se encontró una fuente del sistema para este alfabeto; algunos caracteres pueden verse como cuadros.",
            "fin del mensaje",
            "fin del contacto",
            "nuevo párrafo",
            "cambio, a una estación concreta",
            "espere",
            "comience a copiar",
        ],
        Lang::Fr => [
            "Traducteur de code Morse",
            "Texte → Morse",
            "Morse → Texte",
            "Texte :",
            "Morse (lettres séparées par des espaces, / entre les mots) :",
            "Alphabet :",
            "Détection automatique",
            "Prosignes :",
            "Résultat :",
            "Copier",
            "Copié dans le presse-papiers.",
            "Vitesse de transmission",
            "Vitesse des caractères :",
            "Méthode Farnsworth",
            "Envoie les caractères à pleine vitesse mais allonge les pauses entre lettres et mots : la méthode recommandée par l’ARRL et la CW Academy pour apprendre le Morse, car elle évite l’habitude de « compter les ti et les ta » qui plafonne ensuite la vitesse.",
            "Vitesse effective :",
            "Transmettre",
            "Astuce : essayez d’abord « SOS », ou cliquez sur un prosigne comme <AR> pour l’ajouter au texte.",
            "Langue :",
            "Aucune police système trouvée pour cet alphabet ; certains caractères peuvent s’afficher sous forme de carrés.",
            "fin de message",
            "fin de contact",
            "nouveau paragraphe",
            "à vous, station désignée",
            "attendez",
            "début de transmission",
        ],
        Lang::De => [
            "Morsecode-Übersetzer",
            "Text → Morse",
            "Morse → Text",
            "Text:",
            "Morse (Zeichen durch Leerzeichen getrennt, / zwischen Wörtern):",
            "Alphabet:",
            "Automatisch erkennen",
            "Betriebszeichen:",
            "Ergebnis:",
            "Kopieren",
            "In die Zwischenablage kopiert.",
            "Sendegeschwindigkeit",
            "Zeichengeschwindigkeit:",
            "Farnsworth-Timing",
            "Sendet die Zeichen mit voller Geschwindigkeit, verlängert aber die Pausen zwischen Buchstaben und Wörtern – die von ARRL und CW Academy empfohlene Lernmethode, weil sie das „Punkte-und-Striche-Zählen“ verhindert, das später die Geschwindigkeit begrenzt.",
            "Effektive Geschwindigkeit:",
            "Senden",
            "Tipp: Probiere zuerst „SOS“ oder klicke auf ein Betriebszeichen wie <AR>, um es einzufügen.",
            "Sprache:",
            "Keine Systemschrift für dieses Alphabet gefunden; manche Zeichen werden eventuell als Kästchen angezeigt.",
            "Ende der Nachricht",
            "Ende der Verbindung",
            "neuer Absatz",
            "Übergabe an eine bestimmte Station",
            "warten",
            "Beginn der Übermittlung",
        ],
        Lang::It => [
            "Traduttore di codice Morse",
            "Testo → Morse",
            "Morse → Testo",
            "Testo:",
            "Morse (lettere separate da spazi, / tra le parole):",
            "Alfabeto:",
            "Rilevamento automatico",
            "Prosegni:",
            "Risultato:",
            "Copia",
            "Copiato negli appunti.",
            "Velocità di trasmissione",
            "Velocità dei caratteri:",
            "Temporizzazione Farnsworth",
            "Invia i caratteri a piena velocità ma allunga le pause tra lettere e parole: il metodo consigliato da ARRL e CW Academy per imparare il Morse, perché evita l’abitudine di «contare punti e linee» che in seguito limita la velocità.",
            "Velocità effettiva:",
            "Trasmetti",
            "Suggerimento: prova prima «SOS» oppure fai clic su un prosegno come <AR> per aggiungerlo al testo.",
            "Lingua:",
            "Nessun font di sistema trovato per questo alfabeto; alcuni caratteri potrebbero apparire come riquadri.",
            "fine del messaggio",
            "fine del collegamento",
            "nuovo paragrafo",
            "passo a una stazione specifica",
            "attendere",
            "inizio trasmissione",
        ],
        Lang::Pt => [
            "Tradutor de código Morse",
            "Texto → Morse",
            "Morse → Texto",
            "Texto:",
            "Morse (letras separadas por espaços, / entre palavras):",
            "Alfabeto:",
            "Deteção automática",
            "Prosinais:",
            "Resultado:",
            "Copiar",
            "Copiado para a área de transferência.",
            "Velocidade de transmissão",
            "Velocidade dos caracteres:",
            "Temporização Farnsworth",
            "Envia os caracteres à velocidade máxima, mas alonga as pausas entre letras e palavras — o método recomendado pela ARRL e pela CW Academy para aprender Morse, pois evita o hábito de «contar pontos e traços» que depois limita a velocidade.",
            "Velocidade efetiva:",
            "Transmitir",
            "Dica: experimente primeiro «SOS» ou clique num prosinal como <AR> para o adicionar ao texto.",
            "Idioma:",
            "Nenhum tipo de letra do sistema encontrado para este alfabeto; alguns caracteres podem aparecer como quadrados.",
            "fim da mensagem",
            "fim do contacto",
            "novo parágrafo",
            "câmbio, para uma estação específica",
            "aguarde",
            "início da transmissão",
        ],
        Lang::Ru => [
            "Переводчик азбуки Морзе",
            "Текст → Морзе",
            "Морзе → Текст",
            "Текст:",
            "Морзе (буквы через пробел, / между словами):",
            "Алфавит:",
            "Автоопределение",
            "Процедурные сигналы:",
            "Результат:",
            "Копировать",
            "Скопировано в буфер обмена.",
            "Скорость передачи",
            "Скорость знаков:",
            "Метод Фарнсворта",
            "Знаки передаются на полной скорости, а паузы между буквами и словами удлиняются — метод, рекомендованный ARRL и CW Academy для изучения азбуки Морзе: он не даёт привыкнуть «считать точки и тире», что потом ограничивает скорость.",
            "Эффективная скорость:",
            "Передать",
            "Совет: сначала попробуйте «SOS» или нажмите процедурный сигнал, например <AR>, чтобы добавить его в текст.",
            "Язык:",
            "Системный шрифт для этого алфавита не найден; некоторые символы могут отображаться квадратами.",
            "конец сообщения",
            "конец связи",
            "новый абзац",
            "приём, конкретной станции",
            "ждите",
            "начало передачи",
        ],
        Lang::Uk => [
            "Перекладач азбуки Морзе",
            "Текст → Морзе",
            "Морзе → Текст",
            "Текст:",
            "Морзе (літери через пробіл, / між словами):",
            "Абетка:",
            "Автовизначення",
            "Процедурні сигнали:",
            "Результат:",
            "Копіювати",
            "Скопійовано до буфера обміну.",
            "Швидкість передавання",
            "Швидкість знаків:",
            "Метод Фарнсворта",
            "Знаки передаються на повній швидкості, а паузи між літерами й словами подовжуються — метод, який ARRL і CW Academy радять для вивчення азбуки Морзе: він не дає звикнути «рахувати крапки й тире», що згодом обмежує швидкість.",
            "Ефективна швидкість:",
            "Передати",
            "Порада: спершу спробуйте «SOS» або натисніть процедурний сигнал, наприклад <AR>, щоб додати його до тексту.",
            "Мова:",
            "Системний шрифт для цієї абетки не знайдено; деякі символи можуть відображатися квадратами.",
            "кінець повідомлення",
            "кінець зв’язку",
            "новий абзац",
            "прийом, конкретній станції",
            "чекайте",
            "початок передавання",
        ],
        Lang::El => [
            "Μεταφραστής κώδικα Μορς",
            "Κείμενο → Μορς",
            "Μορς → Κείμενο",
            "Κείμενο:",
            "Μορς (γράμματα με κενό, / ανάμεσα σε λέξεις):",
            "Αλφάβητο:",
            "Αυτόματη ανίχνευση",
            "Διαδικαστικά σήματα:",
            "Αποτέλεσμα:",
            "Αντιγραφή",
            "Αντιγράφηκε στο πρόχειρο.",
            "Ταχύτητα μετάδοσης",
            "Ταχύτητα χαρακτήρων:",
            "Χρονισμός Farnsworth",
            "Στέλνει τους χαρακτήρες με πλήρη ταχύτητα αλλά επιμηκύνει τις παύσεις ανάμεσα σε γράμματα και λέξεις — η μέθοδος που συνιστούν η ARRL και η CW Academy για την εκμάθηση του Μορς, επειδή αποτρέπει τη συνήθεια να «μετράτε τελείες και παύλες», που αργότερα περιορίζει την ταχύτητα.",
            "Πραγματική ταχύτητα:",
            "Μετάδοση",
            "Συμβουλή: δοκιμάστε πρώτα «SOS» ή πατήστε ένα διαδικαστικό σήμα όπως <AR> για να το προσθέσετε στο κείμενο.",
            "Γλώσσα:",
            "Δεν βρέθηκε γραμματοσειρά συστήματος για αυτό το αλφάβητο· ορισμένοι χαρακτήρες μπορεί να εμφανίζονται ως τετράγωνα.",
            "τέλος μηνύματος",
            "τέλος επικοινωνίας",
            "νέα παράγραφος",
            "σειρά σας, συγκεκριμένος σταθμός",
            "αναμονή",
            "έναρξη μετάδοσης",
        ],
        Lang::Ja => [
            "モールス信号翻訳",
            "テキスト → モールス",
            "モールス → テキスト",
            "テキスト：",
            "モールス（文字は空白で区切り、単語の間は /）：",
            "符号表：",
            "自動判別",
            "略符号：",
            "結果：",
            "コピー",
            "クリップボードにコピーしました。",
            "送信速度",
            "文字速度：",
            "ファーンズワース方式",
            "文字は本来の速さで送り、文字間・単語間の間隔だけを長くします。ARRL と CW Academy が推奨する学習法で、「短点と長点を数える」癖がつくのを防ぎ、後で速度が伸び悩むのを避けられます。",
            "実効速度：",
            "送信",
            "ヒント：まず「SOS」を試すか、<AR> などの略符号をクリックしてテキストに追加してください。",
            "言語：",
            "この文字体系のシステムフォントが見つかりません。一部の文字が四角で表示される場合があります。",
            "通信文の終わり",
            "交信の終わり",
            "新しい段落",
            "指定局へどうぞ",
            "待て",
            "送信開始",
        ],
        Lang::Ko => [
            "모스 부호 번역기",
            "텍스트 → 모스",
            "모스 → 텍스트",
            "텍스트:",
            "모스 부호 (글자는 공백으로, 단어 사이는 /):",
            "부호 체계:",
            "자동 감지",
            "절차 부호:",
            "결과:",
            "복사",
            "클립보드에 복사했습니다.",
            "전송 속도",
            "문자 속도:",
            "판스워스 방식",
            "문자는 제 속도로 보내고 글자와 단어 사이의 간격만 늘립니다. ARRL과 CW Academy가 권장하는 학습법으로, 나중에 속도를 가로막는 '점과 선 세기' 습관이 생기지 않게 해 줍니다.",
            "실효 속도:",
            "전송",
            "팁: 먼저 \"SOS\"를 입력해 보거나 <AR> 같은 절차 부호를 눌러 텍스트에 추가해 보세요.",
            "언어:",
            "이 문자 체계에 맞는 시스템 글꼴을 찾지 못했습니다. 일부 문자가 네모로 표시될 수 있습니다.",
            "통신문 끝",
            "교신 끝",
            "새 단락",
            "지정 국에 송신 넘김",
            "대기",
            "송신 시작",
        ],
        Lang::Zh => [
            "摩尔斯电码翻译器",
            "文本 → 摩尔斯",
            "摩尔斯 → 文本",
            "文本：",
            "摩尔斯电码（字母之间用空格，单词之间用 /）：",
            "字母表：",
            "自动检测",
            "勤务符号：",
            "结果：",
            "复制",
            "已复制到剪贴板。",
            "发送速度",
            "字符速度：",
            "法恩斯沃思计时",
            "以正常速度发送字符，但拉长字母和单词之间的停顿。这是 ARRL 和 CW Academy 推荐的摩尔斯学习方法，可避免养成“数点和划”的习惯，否则以后速度难以提高。",
            "有效速度：",
            "发送",
            "提示：先试试“SOS”，或点击 <AR> 等勤务符号将其加入文本。",
            "语言：",
            "未找到适用于该字母表的系统字体，部分字符可能显示为方框。",
            "报文结束",
            "通联结束",
            "新段落",
            "请指定电台发送",
            "请等待",
            "开始发送",
        ],
    };
    row[Msg::ALL
        .iter()
        .position(|m| *m == msg)
        .expect("every Msg is in Msg::ALL")]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_translates_every_message() {
        for lang in Lang::ALL {
            for msg in Msg::ALL {
                assert!(!tr(lang, msg).trim().is_empty(), "{lang:?} {msg:?}");
            }
        }
    }

    #[test]
    fn prosign_placeholder_survives_translation() {
        // The tip tells users to click "<AR>"; every translation must keep
        // the literal token so it matches the button label.
        for lang in Lang::ALL {
            assert!(tr(lang, Msg::Tip).contains("<AR>"), "{lang:?}");
        }
    }

    #[test]
    fn locale_strings_map_to_languages() {
        assert_eq!(Lang::from_locale("de_DE.UTF-8"), Some(Lang::De));
        assert_eq!(Lang::from_locale("pt-BR"), Some(Lang::Pt));
        assert_eq!(Lang::from_locale("zh_CN"), Some(Lang::Zh));
        assert_eq!(Lang::from_locale("en_GB.UTF-8"), Some(Lang::En));
        assert_eq!(Lang::from_locale("ar_EG"), None);
        assert_eq!(Lang::from_locale("C"), None);
    }
}
