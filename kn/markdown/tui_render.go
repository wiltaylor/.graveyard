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

  return ""
}
