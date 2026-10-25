use std::fmt::Display;
use std::iter::Peekable;
use std::str::Chars;

fn placeholder(characters: &mut Peekable<Chars<'_>>) -> Option<String> {
    let mut name = String::new();
    for character in characters.by_ref() {
        match character {
            '}' => return Some(name),
            '{' => return None,
            _ => name.push(character),
        }
    }
    None
}

struct MessageArguments<'a> {
    values: &'a [&'a dyn Display],
    count: Option<u64>,
    next_index: usize,
}

impl MessageArguments<'_> {
    fn next_value(&mut self, name: &str) -> Option<String> {
        match name {
            "n" => self.count.map(|count| count.to_string()),
            "" => {
                let argument = self.values.get(self.next_index).map(ToString::to_string);
                self.next_index += 1;
                argument
            }
            index => index
                .parse::<usize>()
                .ok()
                .and_then(|index| self.values.get(index).map(ToString::to_string)),
        }
    }
}

pub fn format_message(template: &str, arguments: &[&dyn Display], count: Option<u64>) -> String {
    let mut formatted = String::with_capacity(template.len());
    let mut characters = template.chars().peekable();
    let mut message_arguments = MessageArguments {
        values: arguments,
        count,
        next_index: 0,
    };
    while let Some(character) = characters.next() {
        match (character, characters.peek()) {
            ('{', Some('{')) | ('}', Some('}')) => {
                characters.next();
                formatted.push(character);
            }
            ('{', _) => {
                let name = placeholder(&mut characters).unwrap_or_default();
                let value = message_arguments.next_value(&name);
                formatted.push_str(&value.unwrap_or_default());
            }
            _ => formatted.push(character),
        }
    }
    formatted
}
