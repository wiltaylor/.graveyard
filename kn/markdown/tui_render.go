package markdown

import (
  "strings" 
  "fmt"
)

type tokenParser struct {
  tok tokenizer
  eof bool
  level1Ordinal int
  level2Ordinal int
  level3Ordinal int
  lastWasLF bool
}

func NewTokenParser(tok tokenizer) tokenParser {
  return tokenParser{tok: tok}
}

func (p *tokenParser) AtEnd() bool {
  return p.eof
}

func (p *tokenParser) ParseToken() string {

  tok := p.tok.NextToken()

  if tok.Type == TOK_EOF {
    p.eof = true
    return ""
  }

  if tok.Type == TOK_NEWLINE {
    if p.lastWasLF {
      p.level1Ordinal = 0
      p.level2Ordinal = 0
      p.level3Ordinal = 0
    }

    p.lastWasLF = true
    return "\n"
  }

  p.lastWasLF = false

  if tok.Type == TOK_HEADING {

    result := "[blue::b]"
    result += strings.Repeat("", tok.Level)
    result += " " + tok.Text
    result += "[-:-:-]"

    return result
  }



  if tok.Type == TOK_TEXT {
    return tok.Text
  }

  if tok.Type == TOK_BULLET {
    result := ""
    switch tok.Level {
    case 1:
       result = " [green]ﱣ[-] "
    case 2:
       result = "   [green]ﱤ[-] "
    case 3:
       result = "     [green][-] "
    }

    result += tok.Text
    return result
  }

  if tok.Type == TOK_ORDEREDITEM {
    result := ""
    switch tok.Level {
    case 1:
      p.level1Ordinal++
      p.level2Ordinal = 0
      p.level3Ordinal = 0
      result = fmt.Sprintf(" [green]%02d)[-] ", p.level1Ordinal)
    case 2:
      if p.level1Ordinal == 0 {
        p.level1Ordinal = 1
      }
      p.level2Ordinal++
      p.level3Ordinal = 0
      result = fmt.Sprintf(" [green]%02d.%02d)[-] ", p.level1Ordinal, p.level2Ordinal)
    case 3:
      if p.level1Ordinal == 0 {
        p.level1Ordinal = 1
      }
      if p.level2Ordinal == 0 {
        p.level2Ordinal = 1
      }
      p.level3Ordinal++

      result = fmt.Sprintf(" [green]%02d.%02d.%02d)[-] ", p.level1Ordinal, p.level2Ordinal, p.level3Ordinal)
    }

    result += tok.Text

    return result
  }

  return ""
}
