package markdown

import (
	"reflect"
	"testing"
)

func TestMarkdownParser(t *testing.T){
  t.Run("Can parse Heading blocks", func(t *testing.T) {
    cases := []struct {
      markdown string
      expected []tokenType
      level int
      text string
    }{
      {
        markdown: "No Heading",
        expected: []tokenType{TOK_TEXT},
        level: 0,
        text: "",
      },
      {
        markdown: "# Heading #1",
        expected: []tokenType{TOK_HEADING},
        level: 1,
        text: "Heading #1",
      },
      {
        markdown: "## Heading #2",
        expected: []tokenType{TOK_HEADING},
        level: 2,
        text: "Heading #2",
      },
      {
        markdown: "### Heading #3",
        expected: []tokenType{TOK_HEADING},
        level: 3,
        text: "Heading #3",
      },
      {
        markdown: "#### Heading #4",
        expected: []tokenType{TOK_HEADING},
        level: 4,
        text: "Heading #4",
      },   
      {
        markdown: "##### Heading #5",
        expected: []tokenType{TOK_HEADING},
        level: 5,
        text: "Heading #5",
      },
      {
        markdown: "###### Heading #6",
        expected: []tokenType{TOK_HEADING},
        level: 6,
        text: "Heading #6",
      },
    }

    for _, c := range cases {
      parser := newParser(c.markdown)
      got := make([]tokenType, 0)
      level := 0
      txt := ""

      for i := 0; i < len(c.expected); i++ {
        tok := parser.nextToken()
        got = append(got, tok.Type)
       
        if tok.Type == TOK_HEADING {
          level = tok.Level
          txt = tok.Text
        }
      }

      if !reflect.DeepEqual(c.expected, got) {
        t.Errorf("Expected %+v, got %+v", c.expected, got)
      }

      if level != c.level {
        t.Errorf("Expected level to be %+v, got %+v", c.level, level)
      }

      if txt != c.text {
        t.Errorf("Expected header text to be '%+v', got '%+v'", c.text, txt)
      }

      eol := parser.nextToken()

      if eol.Type != TOK_EOF {
        t.Errorf("Expected %+v at end of parser, got %+v", TOK_EOF, eol.Type)
      }
    }

  })

  t.Run("Can pull text from line", func(t *testing.T) {
    markdown := "Hello there\nline2"
    expected := []string{"Hello there", "line2"}
    parser := newParser(markdown)
    
    got := parser.nextToken()

    if got.Type != TOK_TEXT {
      t.Errorf("Expected %+v for type but got %+v", TOK_TEXT, got.Type)
    }

    if got.Text != expected[0] {
      t.Errorf("Expected '%+v' for text but got '%+v'", expected[0], got.Text)
    }

    parser.nextToken() //Skip new line
    got = parser.nextToken()

    if got.Type != TOK_TEXT {
      t.Errorf("Expected %+v for type but got %+v", TOK_TEXT, got.Type)
    }

    if got.Text != expected[1] {
      t.Errorf("Expected '%+v' for text but got'%+v'", expected[1], got.Text)
    }

    got = parser.nextToken()

    if got.Type != TOK_EOF {
      t.Errorf("Expected %+v for type but got %+v", TOK_EOF, got.Type)
    }

  })


  t.Run("Can get New Line tokens", func(t *testing.T) {
    markdown := "Hello there\nline2\nline3"
    expected := []tokenType { TOK_TEXT, TOK_NEWLINE, TOK_TEXT, TOK_NEWLINE, TOK_TEXT}

    parser := newParser(markdown)

    for i := 0; i < len(expected); i++ {
      got := parser.nextToken()

      if got.Type != expected[i] {
        t.Errorf("Expected %+v but got %+v index: %d", expected, got.Type, i)
      }
    }

    got := parser.nextToken()
    if got.Type != TOK_EOF {
      t.Errorf("Expected %+v but got %+v at end of ast", TOK_EOF, got.Type)
    }
  })


}

