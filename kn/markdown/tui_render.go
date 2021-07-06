package markdown

import "strings"

func parseToken(tok token) string {

  if tok.Type == TOK_HEADING {

    result := "[blue::b]"
    result += strings.Repeat("", tok.Level)
    result += " " + tok.Text
    result += "[-:-:-]"

    return result
  }

  if tok.Type == TOK_NEWLINE {
    return "\n"
  }

  if tok.Type == TOK_TEXT {
    return tok.Text
  }

  return ""
}
