#!/usr/bin/env python3
"""Usage: translations.py [update] | check [language...]

update  extracts the @tr strings of app/ui and the tr!/tr_plural! strings of app/src and merges
        them into app/translations/<lang>/LC_MESSAGES/cadence.po, keeping existing translations.
check   reports untranslated entries and translations whose placeholders differ from the source.
"""

import re
import sys
from pathlib import Path

WORKSPACE = Path(__file__).resolve().parent.parent
UI_DIR = WORKSPACE / "app" / "ui"
SOURCE_DIR = WORKSPACE / "app" / "src"
TRANSLATIONS_DIR = WORKSPACE / "app" / "translations"
DOMAIN = "cadence"

TWO_FORMS = "nplurals=2; plural=(n != 1);"
TWO_FORMS_ZERO_SINGULAR = "nplurals=2; plural=(n > 1);"
ONE_FORM = "nplurals=1; plural=0;"
CZECH = "nplurals=3; plural=(n==1) ? 0 : (n>=2 && n<=4) ? 1 : 2;"
LATVIAN = "nplurals=3; plural=(n%10==1 && n%100!=11) ? 0 : (n != 0) ? 1 : 2;"
POLISH = "nplurals=3; plural=(n==1) ? 0 : (n%10>=2 && n%10<=4 && (n%100<10 || n%100>=20)) ? 1 : 2;"
ROMANIAN = "nplurals=3; plural=(n==1) ? 0 : (n==0 || (n%100>0 && n%100<20)) ? 1 : 2;"
EAST_SLAVIC = "nplurals=3; plural=(n%10==1 && n%100!=11) ? 0 : (n%10>=2 && n%10<=4 && (n%100<10 || n%100>=20)) ? 1 : 2;"
SLOVENIAN = "nplurals=4; plural=(n%100==1) ? 0 : (n%100==2) ? 1 : (n%100==3 || n%100==4) ? 2 : 3;"
ARABIC = "nplurals=6; plural=(n==0) ? 0 : (n==1) ? 1 : (n==2) ? 2 : (n%100>=3 && n%100<=10) ? 3 : (n%100>=11) ? 4 : 5;"

CLDR_FORMS = {
    TWO_FORMS: ["one", "other"],
    TWO_FORMS_ZERO_SINGULAR: ["one", "other"],
    ONE_FORM: ["other"],
    CZECH: ["one", "few", "other"],
    LATVIAN: ["one", "other", "zero"],
    POLISH: ["one", "few", "many"],
    ROMANIAN: ["one", "few", "other"],
    EAST_SLAVIC: ["one", "few", "many"],
    SLOVENIAN: ["one", "two", "few", "other"],
    ARABIC: ["zero", "one", "two", "few", "many", "other"],
}

LANGUAGES = {
    "ar": ARABIC,
    "bg": TWO_FORMS,
    "ca": TWO_FORMS,
    "cs": CZECH,
    "da": TWO_FORMS,
    "de": TWO_FORMS,
    "es": TWO_FORMS,
    "et": TWO_FORMS,
    "eu": TWO_FORMS,
    "fa": TWO_FORMS_ZERO_SINGULAR,
    "fi": TWO_FORMS,
    "fr": TWO_FORMS_ZERO_SINGULAR,
    "gl": TWO_FORMS,
    "hu": TWO_FORMS,
    "id": ONE_FORM,
    "it": TWO_FORMS,
    "ja": ONE_FORM,
    "ko": ONE_FORM,
    "lv": LATVIAN,
    "nb-NO": TWO_FORMS,
    "nl": TWO_FORMS,
    "nn": TWO_FORMS,
    "pl": POLISH,
    "pt": TWO_FORMS,
    "pt-BR": TWO_FORMS_ZERO_SINGULAR,
    "ro": ROMANIAN,
    "ru": EAST_SLAVIC,
    "sk": CZECH,
    "sl": SLOVENIAN,
    "sr": EAST_SLAVIC,
    "sv": TWO_FORMS,
    "ta": TWO_FORMS,
    "th": ONE_FORM,
    "tl": TWO_FORMS,
    "tr": TWO_FORMS,
    "uk": EAST_SLAVIC,
    "zh-Hans": ONE_FORM,
    "zh-Hant": ONE_FORM,
}

STRING = r'"((?:[^"\\]|\\.)*)"'
SLINT_CALL = re.compile(r"@tr\(\s*" + STRING + r"(?:\s*\|\s*" + STRING + r"\s*%)?")
RUST_SINGULAR = re.compile(r"\btr!\(\s*" + STRING)
RUST_PLURAL = re.compile(r"\btr_plural!\(\s*" + STRING + r"\s*,\s*" + STRING)
PLACEHOLDER = re.compile(r"\{(n|\d*)\}")


def unescape(literal):
    return re.sub(r"\\(.)", lambda match: {"n": "\n", "t": "\t"}.get(match[1], match[1]), literal)


def po_quote(text):
    escaped = text.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n").replace("\t", "\\t")
    return f'"{escaped}"'


def po_unquote(quoted):
    return unescape(quoted.strip()[1:-1])


def is_test_source(path):
    return "tests" in path.parts or path.name in ("tests.rs", "test_support.rs") or path.name.endswith("_tests.rs")


def extract_messages():
    messages = {}
    for path in sorted(UI_DIR.rglob("*.slint")):
        for match in SLINT_CALL.finditer(path.read_text()):
            singular = unescape(match[1])
            plural = unescape(match[2]) if match[2] is not None else ""
            messages[(singular, plural)] = None
    for path in sorted(SOURCE_DIR.rglob("*.rs")):
        if is_test_source(path):
            continue
        text = path.read_text()
        for match in RUST_SINGULAR.finditer(text):
            messages[(unescape(match[1]), "")] = None
        for match in RUST_PLURAL.finditer(text):
            messages[(unescape(match[1]), unescape(match[2]))] = None
    return sorted(messages, key=lambda key: (key[0].casefold(), key))


def po_path(language):
    return TRANSLATIONS_DIR / language / "LC_MESSAGES" / f"{DOMAIN}.po"


def read_po(path):
    if not path.exists():
        return {}
    entries = {}
    current = {}
    field = None

    def flush():
        if "msgid" in current and current["msgid"] != "":
            forms = [current.get(f"msgstr[{index}]", "") for index in range(10) if f"msgstr[{index}]" in current]
            translated = forms if "msgid_plural" in current else [current.get("msgstr", "")]
            entries[(current["msgid"], current.get("msgid_plural", ""))] = translated

    for line in path.read_text().splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        if stripped.startswith('"'):
            if field is not None:
                current[field] += po_unquote(stripped)
            continue
        keyword, _, value = stripped.partition(" ")
        if keyword == "msgid":
            flush()
            current = {}
        field = keyword
        current[field] = po_unquote(value)
    flush()
    return entries


def po_header(language):
    return (
        'msgid ""\nmsgstr ""\n'
        '"Content-Type: text/plain; charset=UTF-8\\n"\n'
        f'"Language: {language}\\n"\n'
        f'"Plural-Forms: {LANGUAGES[language]}\\n"\n'
    )


def form_count(language):
    return len(CLDR_FORMS[LANGUAGES[language]])


def po_entry(language, key, translated):
    singular, plural = key
    lines = [f"msgid {po_quote(singular)}"]
    if not plural:
        lines.append(f"msgstr {po_quote(translated[0] if translated else '')}")
        return "\n".join(lines)
    lines.append(f"msgid_plural {po_quote(plural)}")
    forms = translated if len(translated) == form_count(language) else [""] * form_count(language)
    lines.extend(f"msgstr[{index}] {po_quote(form)}" for index, form in enumerate(forms))
    return "\n".join(lines)


def write_po(language, messages, translations):
    path = po_path(language)
    path.parent.mkdir(parents=True, exist_ok=True)
    entries = [po_entry(language, key, translations.get(key, [])) for key in messages]
    path.write_text("\n\n".join([po_header(language), *entries]) + "\n")


def is_translated(translated):
    return bool(translated) and all(translated)


def update():
    messages = extract_messages()
    for language in LANGUAGES:
        write_po(language, messages, read_po(po_path(language)))
    translated = sum(is_translated(read_po(po_path("de")).get(key, [])) for key in messages)
    print(f"{len(messages)} messages, {len(LANGUAGES)} languages ({translated} translated in de)")


def argument_indexes(text):
    indexes = []
    for match in PLACEHOLDER.finditer(text):
        if match[1] == "n":
            continue
        indexes.append(int(match[1]) if match[1] else len(indexes))
    return sorted(set(indexes))


def placeholder_problems(key, translated):
    singular, plural = key
    problems = []
    for index, form in enumerate(translated):
        source = plural if plural and index > 0 else singular
        if argument_indexes(form) != argument_indexes(source):
            problems.append(f"arguments {argument_indexes(form)} != {argument_indexes(source)}")
        if "{n}" in form and "{n}" not in singular + plural:
            problems.append("unexpected {n}")
        if form.count("{") - form.count("{{") * 2 != form.count("}") - form.count("}}") * 2:
            problems.append("unbalanced braces")
    return problems


def check(languages):
    messages = extract_messages()
    failed = False
    for language in languages or LANGUAGES:
        entries = read_po(po_path(language))
        missing = [key for key in messages if not is_translated(entries.get(key, []))]
        for key in messages:
            for problem in placeholder_problems(key, entries.get(key, [])) if key not in missing else []:
                print(f"{language}: {key[0]!r}: {problem}")
                failed = True
        print(f"{language}: {len(messages) - len(missing)}/{len(messages)} translated")
        failed = failed or bool(missing)
    return 1 if failed else 0


def main(arguments):
    match arguments:
        case [] | ["update"]:
            update()
        case ["check", *languages]:
            return check(languages)
        case _:
            print(__doc__, file=sys.stderr)
            return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
