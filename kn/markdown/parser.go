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

func (p *parser) readToNextToken() string {

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

func(p *parser) parseHeadings() (bool, token) {
  if !p.startOfLine {
    return false, token{}
  }

  for i := 6; i > 0; i-- {
    if p.peekChar(i + 1) == strings.Repeat("#", i) + " " {
      p.advance(i + 1)
      txt := p.readToNextToken()
      p.advance(len(txt))
      return true, token{ Level: i, Text: txt}
    }
  }

  return false, token{}
}

func(p *parser) parseBulletPoints() (bool, token) {
  if !p.startOfLine {
    return false, token{}
  }

  for i := 0; i < 3; i++ {
    for _, b := range []string{"-", "+", "*"} {
      if p.peekChar(i * 2 + 3) == strings.Repeat( " ", i * 2) + " " + b + " " {
        p.advance(i * 2 + 3)
        txt := p.readToNextToken()
        p.advance(len(txt))
        return true, token{ Level: i + 1, Text: txt, Type: TOK_BULLET}
      }
    }
  }


  return false, token{}
}

func(p *parser) parseOrderedList() (bool, token) {
  if !p.startOfLine {
    return false, token{}
  }
  
  for i := 0; i < 3; i++ {
    if p.peekChar(i * 2 + 4) == strings.Repeat(" ", i * 2) + " 1. " {
      p.advance(i * 2 + 4)
      txt := p.readToNextToken()
      p.advance(len(txt))
      return true, token{ Level: i + 1, Text: txt, Type: TOK_ORDEREDITEM}
    }
  }
  return false, token{}
}

func(p *parser) parseCodeBlock() (bool, token) {
  if !p.startOfLine {
    return false, token{}
  }

    if p.peekChar(3) == "```" {
      p.advance(3)

      fenceLeng := 3

      if p.peekChar(1) == "`" {
        fenceLeng = 4
        p.advance(1)
      }

      code := ""
      lang := p.readToNextToken()
      p.advance(len(lang) + 1)

      for {
        line := p.readToEolNoChecking()
        p.advance(len(line) + 1) // eating line breaks

        if line == strings.Repeat("`", fenceLeng) || p.eof {
          break
        }
        code += line + "\n"
      }

      code = strings.TrimSuffix(code, "\n")

      return true, token{Type: TOK_CODEBLOCK, Language: lang, Text: code}
    }
  return false, token{}
}

func(p *parser) parseLink() (bool, token) {
  if p.peekChar(1) == "[" {
    txt := p.readToNextToken()

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
      return true, token{ Type: TOK_LINK, Text: strconv.Itoa(id)} 
    }
  }
  return false, token{}
}

func(p *parser) parseFormatedString() (bool, token) {
  if p.peekChar(1) == "`" {
    txt := p.readToNextToken()
    full := p.peekChar(len(txt) + 1)
    full_len := len(full) - 1
    if full_len < 0 {
      full_len = 0
    }

    if full[full_len:] == "`" {
      txt = txt[1:]
      p.advance(len(full))
      return true, token{ Type: TOK_TEXT, Format: TXT_CODE, Text: txt}
    }
  }
  return false, token{}
}

func (p *parser) parseNewLine() (bool, token) {
  if p.peekChar(1) == "\n" {
    p.advance(1)
    p.startOfLine = true
    return true, token{Type: TOK_NEWLINE}
  }
  return false, token{}
}

func(p *parser) parseEOF() (bool, token) {
  if p.position >= len(p.text) {
    return true, token{ Type: TOK_EOF}
  }
  return false, token{}
}

func(p *parser) parseText() (bool, token) {
  txt := p.readToNextToken()
  p.advance(len(txt))
  return true, token { Type: TOK_TEXT, Text: txt}
}

func(p *parser) NextToken() token {
  return func(f []func()(bool, token))(token) {
    for _, fn := range f {
      handled, tok := fn()

      if handled {
        if tok.Type != TOK_NEWLINE {
          p.startOfLine = false
        }
        return tok
      }
    }

    return token{}
  }([]func()(bool, token){
    p.parseEOF, //Keep this item at the top to guard end of files.
    p.parseNewLine,
    p.parseHeadings,
    p.parseBulletPoints,
    p.parseOrderedList,
    p.parseCodeBlock,
    p.parseLink,
    p.parseFormatedString,
    p.parseText, //Keep this item at the bottom to catch all remaining text
  })
}
