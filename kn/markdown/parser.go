package markdown

import (
	"strings"
)

const (
  TOK_HEADING tokenType = iota
  TOK_TEXT
  TOK_EOF
  TOK_NEWLINE
)

type tokenType int

type parser struct {
  text string
  position int
  eof bool
}


type token struct {
  Type tokenType
  Level int
  Text string
}

func newParser(markdown string) parser {
  return parser{
    text: markdown,
    position: 0,
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

func (p *parser) readToEol() string {
  idx := strings.Index(p.text[p.position:], "\n")

  if idx == -1 {
    return p.text[p.position:]
  }

  return p.text[p.position:p.position + idx]
}

func(p *parser) nextToken() token {
  if p.position >= len(p.text) {
    return token{ Type: TOK_EOF}
  }

  if p.peekChar(1) == "\n" {
    p.advance(1)
    return token{Type: TOK_NEWLINE}
  }

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

  txt := p.readToEol()

  p.advance(len(txt)) //Eat line end

  return token { Type: TOK_TEXT, Text: txt}
}
