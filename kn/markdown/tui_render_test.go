package markdown

import "testing"

type fakeTokenizer struct {
  toks []token
  links []link
  index int
}

func(t *fakeTokenizer) NextToken() token {
  if t.index >= len(t.toks) {
    return token{Type: TOK_EOF}
  }

  t.index++
  return t.toks[t.index - 1]
}

func(t *fakeTokenizer) Links() []link {
  return t.links
}

func TestTuiRender(t *testing.T) {
  t.Run("Can convert tokens", func(t *testing.T) {
    cases := []struct{
      tok token
      expected string
    }{
      {
        tok: token{
          Type: TOK_NEWLINE,
        },
        expected: "\n",
      },
      {
        tok: token{
           Type: TOK_TEXT,
           Level: 0,
           Text: "Hi",
          },
        expected: "Hi",
      },
      {
        tok: token{
           Type: TOK_HEADING,
           Level: 1,
           Text: "Hi",
          },
        expected: "[blue::b] Hi[-:-:-]",
      },
      {
        tok: token{
           Type: TOK_HEADING,
           Level: 2,
           Text: "Hi",

          },
        expected: "[blue::b] Hi[-:-:-]",
      },
      {
        tok: token{
           Type: TOK_HEADING,
           Level: 3,
           Text: "Hi",

          },
        expected: "[blue::b] Hi[-:-:-]",
      },    
      {
        tok: token{
           Type: TOK_HEADING,
           Level: 4,
           Text: "Hello",

          },
        expected: "[blue::b] Hello[-:-:-]",
      },
      {
        tok: token{
           Type: TOK_HEADING,
           Level: 5,
           Text: "Hi",

          },
        expected: "[blue::b] Hi[-:-:-]",
      },
      {
        tok: token{
           Type: TOK_HEADING,
           Level: 6,
           Text: "Hi",

          },
        expected: "[blue::b] Hi[-:-:-]",
      },
      {
        tok : token{
          Type: TOK_BULLET,
          Level: 1,
          Text: "Hi",
        },
        expected: " [green]ﱣ[-] Hi",
      },
      {
        tok : token{
          Type: TOK_BULLET,
          Level: 2,
          Text: "Hi",
        },
        expected: "   [green]ﱤ[-] Hi",
      },
      {
        tok : token{
          Type: TOK_BULLET,
          Level: 3,
          Text: "Hi",
        },
        expected: "     [green][-] Hi",
      },
      {
        tok : token{
          Type: TOK_ORDEREDITEM,
          Level: 1,
          Text: "Hi",
        },
        expected: " [green]01)[-] Hi",
      },
      {
        tok : token{
          Type: TOK_ORDEREDITEM,
          Level: 2,
          Text: "Hi",
        },
        expected: " [green]01.01)[-] Hi",
      },
      {
        tok : token{
          Type: TOK_ORDEREDITEM,
          Level: 3,
          Text: "Hi",
        },
        expected: " [green]01.01.01)[-] Hi",
      },
      {
        tok: token{
          Type: TOK_TEXT,
          Format: TXT_CODE,
          Text: "Hey",
        },
        expected: "[green]Hey[-:-:-]",
      },
      {
        tok: token{
          Type: TOK_CODEBLOCK,
          Text: "woo code",
        },
        expected: "[green:gray]woo code\n[-:-:-]",
      },
    }

    for _, c := range cases {
      tok := fakeTokenizer{ toks: []token{c.tok}}
      parser := NewTokenParser(&tok)
      got := parser.ParseToken()

      if got != c.expected {
        t.Errorf("Expected '%+v' but got '%+v'", c.expected, got)
      }
    }
  })

 t.Run("Orderd lists have ordered numbers", func(t *testing.T) {
    cases := []struct {
      tokens []token
      text []string
    }{
      {
        tokens: []token{
          {Type: TOK_ORDEREDITEM, Level: 1, Text: "One"}, {Type: TOK_NEWLINE},
          {Type: TOK_ORDEREDITEM, Level: 1, Text: "Two"}, {Type: TOK_NEWLINE},
          {Type: TOK_ORDEREDITEM, Level: 1, Text: "Three"},
        },
        text: []string{ " [green]01)[-] One", "\n", " [green]02)[-] Two", "\n", " [green]03)[-] Three" },
      },
      {
        tokens: []token{
          {Type: TOK_ORDEREDITEM, Level: 1, Text: "One"}, {Type: TOK_NEWLINE},
          {Type: TOK_ORDEREDITEM, Level: 2, Text: "One"}, {Type: TOK_NEWLINE},
          {Type: TOK_ORDEREDITEM, Level: 2, Text: "Two"},
        },
        text: []string{ " [green]01)[-] One", "\n", " [green]01.01)[-] One", "\n", " [green]01.02)[-] Two" },
      },
      {
        tokens: []token{
          {Type: TOK_ORDEREDITEM, Level: 1, Text: "One"}, {Type: TOK_NEWLINE},
          {Type: TOK_ORDEREDITEM, Level: 2, Text: "One"}, {Type: TOK_NEWLINE},
          {Type: TOK_ORDEREDITEM, Level: 3, Text: "One"}, {Type: TOK_NEWLINE},
          {Type: TOK_ORDEREDITEM, Level: 3, Text: "Two"},
        },
        text: []string{ " [green]01)[-] One", "\n", " [green]01.01)[-] One", "\n",
            " [green]01.01.01)[-] One", "\n", " [green]01.01.02)[-] Two" },
      },
      {
        tokens: []token{
          {Type: TOK_ORDEREDITEM, Level: 1, Text: "One"}, {Type: TOK_NEWLINE},
          {Type: TOK_ORDEREDITEM, Level: 1, Text: "Two"}, {Type: TOK_NEWLINE},
          {Type: TOK_NEWLINE},
          {Type: TOK_ORDEREDITEM, Level: 1, Text: "One"},
        },
        text: []string{ " [green]01)[-] One", "\n", " [green]02)[-] Two", "\n","\n", " [green]01)[-] One" },
      },
      {
        tokens: []token{
          {Type: TOK_ORDEREDITEM, Level: 1, Text: "One"}, {Type: TOK_NEWLINE},
          {Type: TOK_ORDEREDITEM, Level: 2, Text: "One"}, {Type: TOK_NEWLINE},
          {Type: TOK_ORDEREDITEM, Level: 2, Text: "Two"}, {Type: TOK_NEWLINE},
          {Type: TOK_ORDEREDITEM, Level: 3, Text: "One"}, {Type: TOK_NEWLINE},
          {Type: TOK_ORDEREDITEM, Level: 3, Text: "Two"}, {Type: TOK_NEWLINE},
          {Type: TOK_ORDEREDITEM, Level: 1, Text: "One"}, {Type: TOK_NEWLINE},
          {Type: TOK_ORDEREDITEM, Level: 2, Text: "One"}, {Type: TOK_NEWLINE},
          {Type: TOK_ORDEREDITEM, Level: 2, Text: "Two"}, {Type: TOK_NEWLINE},
          {Type: TOK_ORDEREDITEM, Level: 3, Text: "One"}, {Type: TOK_NEWLINE},
          {Type: TOK_ORDEREDITEM, Level: 3, Text: "Two"},
        },
        text: []string{ " [green]01)[-] One", "\n", " [green]01.01)[-] One", "\n",
        " [green]01.02)[-] Two", "\n", " [green]01.02.01)[-] One", "\n", " [green]01.02.02)[-] Two", "\n",
        " [green]02)[-] One", "\n", " [green]02.01)[-] One", "\n"," [green]02.02)[-] Two", "\n",
        " [green]02.02.01)[-] One", "\n", " [green]02.02.02)[-] Two", },
      },

    }

    for _, c := range cases {
      tok := fakeTokenizer { toks: c.tokens }
      parser := NewTokenParser(&tok)

      for i := 0; i < len(c.text); i++ {
        got := parser.ParseToken()

        if got != c.text[i] {
          t.Errorf("Expected '%+v' but got '%+v' index %d", c.text[i], got, i)
        }
      }

    }

  })

  t.Run("Can render links", func(t *testing.T) {
    cases := []struct{
      tok token
      link link
      expected string
    }{
      {
        tok: token{
          Type: TOK_LINK,
          Text: "0",
        },
        link: link{
          Type: LNK_URL,
          Target: "http://www.google.com",
          Title: "WebLink",
        },
        expected: `["0"][blue::u]WebLink[-:-:-][""]`,
      },
      {
        tok: token{
          Type: TOK_LINK,
          Text: "0",
        },
        link: link{
          Type: LNK_ZK,
          Target: "1234",
          Title: "ZKLink",
        },
        expected: `["0"][blue::u]ZKLink[-:-:-][""]`,
      },
      {
        tok: token{
          Type: TOK_LINK,
          Text: "0",
        },
        link: link{
          Type: LNK_ZKA,
          Target: "1234",
          Title: "ZKALink",
        },
        expected: `["0"][blue::u]ZKALink[-:-:-][""]`,
      },
      {
        tok: token{
          Type: TOK_LINK,
          Text: "0",
        },
        link: link{
          Type: LNK_REPORT,
          Target: "1234",
          Title: "ReportLink",
        },
        expected: `["0"][blue::u]ReportLink[-:-:-][""]`,
      },
      {
        tok: token{
          Type: TOK_LINK,
          Text: "0",
        },
        link: link{
          Type: LNK_EMPTY,
          Target: "",
          Title: "EmptyLink",
        },
        expected: `["0"][blue::u]EmptyLink[-:-:-][""]`,
      },
    }

    for _, c := range cases {
      tok := fakeTokenizer { toks: []token{c.tok}, links: []link{c.link} }
      parser := NewTokenParser(&tok)

      got := parser.ParseToken()

      if got != c.expected {
        t.Errorf("Expected '%+v' got '%+v'", c.expected, got) 
      }
    }

  })
}
