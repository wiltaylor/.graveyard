package markdown

import "testing"

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
    }

    for _, c := range cases {
      got := parseToken(c.tok)

      if got != c.expected {
        t.Errorf("Expected '%+v' but got '%+v'", c.expected, got)
      }
    }
  })
}
