package markdown

func MarkdownToTui(markdown string) string {
  result := ""

  parser := newParser(markdown)

  for {
    tok := parser.nextToken()

    if tok.Type == TOK_EOF {
      break
    }

    result += parseToken(tok)

  }

  return result
}
