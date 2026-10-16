//! The subset of Raven's menu-file language that status HUDs use.
//!
//! OpenJK `codemp/ui/ui_shared.c` reads `.menu` files with the botlib
//! precompiler and a keyword table per block (`menuParseKeywords`,
//! `itemParseKeywords`). HUD files only need window geometry, colours and
//! shaders, so this reader keeps `name`, `rect`, `visible`, `style`,
//! `background`, `forecolor` and `backcolor` of every `menuDef`/`itemDef`
//! and the `loadMenu` lists of the `cg_hudFiles` `.txt` files. Every other
//! keyword is skipped with its numeric, string and `{ ... }` arguments, so
//! scripts written for the full menu system still load. `#include` names
//! are returned for the loader to splice in; other directives are ignored.

/// One parsed `menuDef`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MenuDef {
    pub(crate) window: Window,
    /// `fullScreen`: the menu's background covers the whole screen.
    pub(crate) full_screen: bool,
    pub(crate) items: Vec<Window>,
}

/// The window fields shared by menus and items (`windowDef_t`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Window {
    pub(crate) name: String,
    /// `x y w h` in the 640x480 screen; an item's is relative to its menu.
    /// A negative width mirrors the picture, as the retail right HUD does.
    pub(crate) rect: [f32; 4],
    /// `visible 1`: painted by `Menu_Paint`.
    pub(crate) visible: bool,
    /// `WINDOW_STYLE_*`: 1 filled, 3 shader; others paint no background.
    pub(crate) style: i32,
    pub(crate) background: Option<String>,
    /// `forecolor`; `Window_Init` starts it white.
    pub(crate) fore_color: [f32; 4],
    /// Whether `forecolor` was given (`WINDOW_FORECOLORSET`).
    pub(crate) fore_color_set: bool,
    /// `backcolor`; `Window_Init` starts it transparent black.
    pub(crate) back_color: [f32; 4],
}

impl Default for Window {
    fn default() -> Self {
        Self {
            name: String::new(),
            rect: [0.0; 4],
            visible: false,
            style: 0,
            background: None,
            fore_color: [1.0; 4],
            fore_color_set: false,
            back_color: [0.0; 4],
        }
    }
}

/// Everything one file contributed.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct ParsedFile {
    pub(crate) menus: Vec<MenuDef>,
    /// `loadMenu { "file" ... }` entries, in order.
    pub(crate) load: Vec<String>,
    /// `#include "file"` directives, in order.
    pub(crate) includes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
enum Token<'a> {
    Word(&'a str),
    Text(&'a str),
    Open,
    Close,
    Include(&'a str),
}

/// Split `source` into words, quoted strings and braces; comments and
/// preprocessor lines other than `#include` are dropped.
fn tokenize(source: &str) -> Vec<Token<'_>> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte.is_ascii_whitespace() || matches!(byte, b',' | b';' | b'(' | b')') {
            index += 1;
        } else if bytes[index..].starts_with(b"//") {
            index = line_end(bytes, index);
        } else if bytes[index..].starts_with(b"/*") {
            index = source[index + 2..]
                .find("*/")
                .map_or(bytes.len(), |end| index + 2 + end + 2);
        } else if byte == b'#' {
            let end = line_end(bytes, index);
            let line = &source[index + 1..end];
            if let Some(rest) = line.trim_start().strip_prefix("include") {
                let name = rest
                    .trim()
                    .trim_matches(|c| c == '"' || c == '<' || c == '>');
                if !name.is_empty() {
                    tokens.push(Token::Include(name));
                }
            }
            index = end;
        } else if byte == b'"' {
            let start = index + 1;
            let end = source[start..]
                .find('"')
                .map_or(bytes.len(), |end| start + end);
            tokens.push(Token::Text(&source[start..end]));
            index = (end + 1).min(bytes.len());
        } else if byte == b'{' {
            tokens.push(Token::Open);
            index += 1;
        } else if byte == b'}' {
            tokens.push(Token::Close);
            index += 1;
        } else {
            let start = index;
            while index < bytes.len()
                && !bytes[index].is_ascii_whitespace()
                && !matches!(bytes[index], b'{' | b'}' | b'"' | b',' | b';' | b'(' | b')')
                && !bytes[index..].starts_with(b"//")
                && !bytes[index..].starts_with(b"/*")
            {
                index += 1;
            }
            tokens.push(Token::Word(&source[start..index]));
        }
    }
    tokens
}

fn line_end(bytes: &[u8], from: usize) -> usize {
    bytes[from..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(bytes.len(), |end| from + end)
}

/// Parse one menu or list file. Never fails: malformed parts are skipped.
pub(crate) fn parse(source: &str) -> ParsedFile {
    let tokens = tokenize(source);
    let mut cursor = Cursor {
        tokens: &tokens,
        index: 0,
    };
    let mut file = ParsedFile::default();
    while let Some(token) = cursor.next() {
        match token {
            Token::Include(name) => file.includes.push(name.to_owned()),
            Token::Word(word) if word.eq_ignore_ascii_case("menuDef") => {
                if cursor.peek() == Some(&Token::Open) {
                    cursor.index += 1;
                    file.menus.push(cursor.menu());
                }
            }
            Token::Word(word) if word.eq_ignore_ascii_case("loadMenu") => {
                if cursor.peek() == Some(&Token::Open) {
                    cursor.index += 1;
                    while let Some(token) = cursor.next() {
                        match token {
                            Token::Close => break,
                            Token::Text(name) | Token::Word(name) => {
                                file.load.push(name.to_owned());
                            }
                            Token::Open => cursor.skip_block(),
                            Token::Include(_) => {}
                        }
                    }
                }
            }
            // Top-level wrappers ("{ menuDef ... }") are transparent;
            // assetGlobalDef and other named blocks are skipped whole.
            Token::Word(_) => {
                if cursor.peek() == Some(&Token::Open) {
                    cursor.index += 1;
                    cursor.skip_block();
                }
            }
            Token::Open | Token::Close | Token::Text(_) => {}
        }
    }
    file
}

struct Cursor<'t, 'a> {
    tokens: &'t [Token<'a>],
    index: usize,
}

impl<'a> Cursor<'_, 'a> {
    fn next(&mut self) -> Option<Token<'a>> {
        let token = self.tokens.get(self.index).cloned();
        self.index += usize::from(token.is_some());
        token
    }

    fn peek(&self) -> Option<&Token<'a>> {
        self.tokens.get(self.index)
    }

    /// Skip to just past the `}` closing a block whose `{` was consumed.
    fn skip_block(&mut self) {
        let mut depth = 1_usize;
        while let Some(token) = self.next() {
            match token {
                Token::Open => depth += 1,
                Token::Close => {
                    depth -= 1;
                    if depth == 0 {
                        return;
                    }
                }
                _ => {}
            }
        }
    }

    /// Skip an unknown keyword's arguments: numbers, strings and blocks.
    fn skip_arguments(&mut self) {
        loop {
            match self.peek() {
                Some(Token::Text(_)) => self.index += 1,
                Some(Token::Word(word)) if word.parse::<f32>().is_ok() => self.index += 1,
                Some(Token::Open) => {
                    self.index += 1;
                    self.skip_block();
                }
                _ => return,
            }
        }
    }

    fn string(&mut self) -> Option<String> {
        match self.peek() {
            Some(Token::Text(text) | Token::Word(text)) => {
                let text = (*text).to_owned();
                self.index += 1;
                Some(text)
            }
            _ => None,
        }
    }

    fn number(&mut self) -> Option<f32> {
        match self.peek() {
            Some(Token::Word(word) | Token::Text(word)) => {
                let value = word.parse::<f32>().ok()?;
                self.index += 1;
                Some(value)
            }
            _ => None,
        }
    }

    fn numbers<const N: usize>(&mut self, fallback: [f32; N]) -> [f32; N] {
        let mut values = fallback;
        for value in &mut values {
            match self.number() {
                Some(number) => *value = number,
                None => break,
            }
        }
        values
    }

    /// Read one window keyword into `window`; false if it is not one.
    fn window_keyword(&mut self, keyword: &str, window: &mut Window) -> bool {
        match keyword.to_ascii_lowercase().as_str() {
            "name" => window.name = self.string().unwrap_or_default(),
            "rect" => window.rect = self.numbers(window.rect),
            "visible" => window.visible = self.number().unwrap_or(0.0) != 0.0,
            "style" => window.style = self.number().unwrap_or(0.0) as i32,
            "background" => {
                window.background = self.string().filter(|name| !name.is_empty());
            }
            "forecolor" => {
                window.fore_color = self.numbers(window.fore_color);
                window.fore_color_set = true;
            }
            "backcolor" => window.back_color = self.numbers(window.back_color),
            _ => return false,
        }
        true
    }

    /// The body of a `menuDef` after its `{`.
    fn menu(&mut self) -> MenuDef {
        let mut menu = MenuDef {
            window: Window::default(),
            full_screen: false,
            items: Vec::new(),
        };
        while let Some(token) = self.next() {
            match token {
                Token::Close => break,
                Token::Word(word) if word.eq_ignore_ascii_case("itemDef") => {
                    if self.peek() == Some(&Token::Open) {
                        self.index += 1;
                        menu.items.push(self.item());
                    }
                }
                Token::Word(word) if word.eq_ignore_ascii_case("fullScreen") => {
                    menu.full_screen = self.number().unwrap_or(0.0) != 0.0;
                }
                Token::Word(word) => {
                    if !self.window_keyword(word, &mut menu.window) {
                        self.skip_arguments();
                    }
                }
                Token::Open => self.skip_block(),
                Token::Text(_) | Token::Include(_) => {}
            }
        }
        menu
    }

    /// The body of an `itemDef` after its `{`.
    fn item(&mut self) -> Window {
        let mut item = Window::default();
        while let Some(token) = self.next() {
            match token {
                Token::Close => break,
                Token::Word(word) => {
                    if !self.window_keyword(word, &mut item) {
                        self.skip_arguments();
                    }
                }
                Token::Open => self.skip_block(),
                Token::Text(_) | Token::Include(_) => {}
            }
        }
        item
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HUD: &str = r#"
// In Game HUD
#include "ui/menudef.h"
assetGlobalDef
{
    bigFont "fonts/reallybigfont" 20
}
{
    menuDef
    {
        name "lefthud"
        fullScreen 0 // MENU_FALSE
        rect 0 368 112 112
        visible 1
        appearanceIncrement 75
        itemDef
        {
            name "frame"
            forecolor 1 1 1 1
            background "gfx/hud/hudleft"
            rect 0 0 112 112
        }
        itemDef
        {
            name health_tic1
            group none
            background "gfx/hud/health_tic_1"
            rect 20 24 28 28
            /* decoration */ visible 1
            action { play "sound/x.wav" ; close lefthud }
        }
    }
    menuDef { name "righthud" rect 640 368 -112 112 itemDef { name ammoamount forecolor 1.0 .658 .062 1 rect -83 98 6 12 } }
}
"#;

    #[test]
    fn reads_hud_menus_and_skips_the_rest() {
        let file = parse(HUD);
        assert_eq!(file.includes, ["ui/menudef.h"]);
        assert_eq!(file.menus.len(), 2);
        let left = &file.menus[0];
        assert_eq!(left.window.name, "lefthud");
        assert_eq!(left.window.rect, [0.0, 368.0, 112.0, 112.0]);
        assert!(left.window.visible && !left.full_screen);
        assert_eq!(left.items.len(), 2);
        assert_eq!(left.items[0].background.as_deref(), Some("gfx/hud/hudleft"));
        assert!(!left.items[0].visible && left.items[0].fore_color_set);
        let tic = &left.items[1];
        assert_eq!(tic.name, "health_tic1");
        assert_eq!(tic.rect, [20.0, 24.0, 28.0, 28.0]);
        assert!(tic.visible && !tic.fore_color_set);
        let right = &file.menus[1];
        assert_eq!(right.window.rect, [640.0, 368.0, -112.0, 112.0]);
        assert_eq!(right.items[0].fore_color, [1.0, 0.658, 0.062, 1.0]);
    }

    #[test]
    fn reads_load_lists() {
        let file =
            parse("// hud menu defs\n{\n\tloadMenu { \"ui/hud.menu\" \"ui/extra.menu\" }\n}\n");
        assert_eq!(file.load, ["ui/hud.menu", "ui/extra.menu"]);
        assert!(file.menus.is_empty());
    }

    #[test]
    fn tolerates_truncated_and_odd_input() {
        let file = parse("menuDef { name \"x\" rect 1 2 itemDef { name");
        assert_eq!(file.menus.len(), 1);
        assert_eq!(file.menus[0].window.rect, [1.0, 2.0, 0.0, 0.0]);
        assert_eq!(file.menus[0].items.len(), 1);
        assert!(parse("}}} \"unterminated").menus.is_empty());
    }
}
