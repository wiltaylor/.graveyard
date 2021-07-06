package markdown

func MarkdownToTui(markdown string) string {
  result := ""

  tokenizer := newParser(markdown)
  parser := NewTokenParser(&tokenizer)
  

  for {
    if parser.AtEnd() {
      break
    }

    result += parser.ParseToken()

  }

  return result
}
