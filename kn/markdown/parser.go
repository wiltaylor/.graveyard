package markdown

import (
	"strconv"
	"strings"
)

const (
  TOK_HEADING tokenType = iota
  TOK_TEXT
  TOK_EOF
  TOK_NEWLINE
  TOK_BULLET
  TOK_ORDEREDITEM
  TOK_LINK
  TOK_CODEBLOCK
)

const (
  LNK_URL linkType = iota
  LNK_ZK
  LNK_ZKA
  LNK_REPORT
  LNK_EMPTY
)

const (
  TXT_PLAIN textFormat = iota
  TXT_CODE
)

type tokenType int
type linkType int
type textFormat int

type link struct {
  Type linkType
  Target string
  Index int
  Title string
}

type parser struct {
  text string
  position int
  eof bool
  links []link
  nextLinkId int
  startOfLine bool
}

type tokenizer interface {
  NextToken() token
  Links() []link
}


type token struct {
  Type tokenType
  Level int
  Text string
  Format textFormat
  Language string
}

func newParser(markdown string) parser {
  return parser{
    text: markdown,
    position: 0,
    startOfLine: true,
  }
}

func (p *parser) peekChar(length int) string {
  if len(p.text) <= p.position + length {
    return ""
  }
    
  return p.text[p.position:p.position + length]
}

func (p *parser) advance(length int) {
  p.position += length
}

func (p *parser) readToEolNoChecking() string {
  idx := strings.Index(p.text[p.position:], "\n")

  txt := ""

  if idx == -1 {
    txt = p.text[p.position:]
  }else{
    txt = p.text[p.position:p.position + idx]
  }

  return txt
}

func (p *parser) readToEol() string {

  idx := strings.Index(p.text[p.position:], "\n")

  txt := ""

  if idx == -1 {
    txt = p.text[p.position:]
  }else{
    txt = p.text[p.position:p.position + idx]
  }

  link := strings.Index(txt, "[")

  if link > 0 {
    txt = txt[:link]
  }

  if len(txt) > 1 {
    code := strings.Index(txt[1:], "`")

    if code > -1 {
      txt = txt[:code + 1]
    }
  }

  return txt
}

func(p *parser) Links() []link {
  return p.links
}

func(p *parser) NextToken() token {
  if p.position >= len(p.text) {
    return token{ Type: TOK_EOF}
  }

  if p.peekChar(1) == "\n" {
    p.advance(1)
    p.startOfLine = true
    return token{Type: TOK_NEWLINE}
  }

  if p.startOfLine {
    p.startOfLine = false
    if p.peekChar(7) == "###### " {
      p.advance(7)
      txt := p.readToEol()
      p.advance(len(txt))
      return token{ Level: 6, Text: txt}
    }

    if p.peekChar(6) == "##### " {
      p.advance(6)
      txt := p.readToEol()
      p.advance(len(txt))
      return token{ Level: 5, Text: txt}
    }

    if p.peekChar(5) == "#### " {
      p.advance(5)
      txt := p.readToEol()
      p.advance(len(txt))
      return token{ Level: 4, Text: txt }
    }

    if p.peekChar(4) == "### " {
      p.advance(4)
      txt := p.readToEol()
      p.advance(len(txt))
      return token{ Level: 3, Text: txt}
    }

    if p.peekChar(3) == "## " {
      p.advance(3)
      txt := p.readToEol()
      p.advance(len(txt))
      return token{ Level: 2, Text: txt}
    }

    if p.peekChar(2) == "# " {
      p.advance(2)
      txt := p.readToEol()
      p.advance(len(txt))
      return token{ Level: 1, Text: txt}
    }

    if p.peekChar(3) == " - " || p.peekChar(3) == " + " || p.peekChar(3) == " * " {
      p.advance(3)
      txt := p.readToEol()
      p.advance(len(txt))
      return token{ Type: TOK_BULLET, Text: txt, Level: 1 }
    }

    if p.peekChar(5) == "   - " || p.peekChar(5) == "   + " || p.peekChar(5) == "   * " {
      p.advance(5)
      txt := p.readToEol()
      p.advance(len(txt))
      return token{ Type: TOK_BULLET, Text: txt, Level: 2 }
    }

    if p.peekChar(7) == "     - " || p.peekChar(7) == "     + " || p.peekChar(7) == "     * " {
      p.advance(7)
      txt := p.readToEol()
      p.advance(len(txt))
      return token{ Type: TOK_BULLET, Text: txt, Level: 3 }
    }

    if p.peekChar(4) == " 1. " {
      p.advance(4)
      txt := p.readToEol()
      p.advance(len(txt))
      return token{ Type: TOK_ORDEREDITEM, Level: 1, Text: txt}
    }

    if p.peekChar(6) == "   1. " {
      p.advance(6)
      txt := p.readToEol()
      p.advance(len(txt))
      return token{ Type: TOK_ORDEREDITEM, Level: 2, Text: txt}
    }

    if p.peekChar(8) == "     1. " {
      p.advance(8)
      txt := p.readToEol()
      p.advance(len(txt))
      return token{ Type: TOK_ORDEREDITEM, Level: 3, Text: txt}
    }

    if p.peekChar(4) == "````" {
      p.advance(4)
      code := ""
      lang := p.readToEol()
      p.advance(len(lang) + 1)

      for {
        line := p.readToEolNoChecking()
        p.advance(len(line) + 1) // eating line breaks

        if line == "````" || p.eof {
          break
        }
        code += line + "\n"
      }

      code = strings.TrimSuffix(code, "\n")

      return token{Type: TOK_CODEBLOCK, Language: lang, Text: code}
    }

    if p.peekChar(3) == "```" {
      p.advance(3)
      code := ""
      lang := p.readToEol()
      p.advance(len(lang) + 1)

      for {
        line := p.readToEolNoChecking()
        p.advance(len(line) + 1) // eating line breaks

        if line == "```" || p.eof {
          break
        }
        code += line + "\n"
      }

      code = strings.TrimSuffix(code, "\n")

      return token{Type: TOK_CODEBLOCK, Language: lang, Text: code}
    }
  }

  if p.peekChar(1) == "[" {
    txt := p.readToEol()

    nextBracket := strings.Index(txt, "]")
    openParen := strings.Index(txt, "(")
    closeParen := strings.Index(txt, ")")

    if nextBracket != -1 && openParen != -1 && closeParen != -1 {
      title := txt[1:nextBracket]
      url := txt[openParen + 1:closeParen]
      urltype := LNK_URL

      if len(url) > 3 && url[:3] == "zk:" {
        url = url[3:]
        urltype = LNK_ZK
      }

      if len(url) > 4 && url[:4] == "zka:" {
        url = url[4:]
        urltype = LNK_ZKA
      }

      if len(url) > 3 && url[:3] == "rp:" {
        url = url[3:]
        urltype = LNK_REPORT
      }

      if strings.Trim(url, " ") == "" {
        urltype = LNK_EMPTY
        url = ""
      }

      p.advance(closeParen + 1)
      id := p.nextLinkId
      p.nextLinkId++
      p.links = append(p.links, link{Title: title, Target: url, Type: urltype, Index: id})
      return token{ Type: TOK_LINK, Text: strconv.Itoa(id)} 
    }
  }

  if p.peekChar(1) == "`" {
    txt := p.readToEol()
    full := p.peekChar(len(txt) + 1)
    full_len := len(full) - 1
    if full_len < 0 {
      full_len = 0
    }

    if full[full_len:] == "`" {
      txt = txt[1:]
      p.advance(len(full) + 1)
      return token{ Type: TOK_TEXT, Format: TXT_CODE, Text: txt}
    }
  }

  txt := p.readToEol()

  p.advance(len(txt))

  return token { Type: TOK_TEXT, Text: txt}
}
