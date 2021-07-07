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
        tok := parser.NextToken()
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

      eol := parser.NextToken()

      if eol.Type != TOK_EOF {
        t.Errorf("Expected %+v at end of parser, got %+v", TOK_EOF, eol.Type)
      }
    }

  })

  t.Run("Can pull text from line", func(t *testing.T) {
    markdown := "Hello there\nline2"
    expected := []string{"Hello there", "line2"}
    parser := newParser(markdown)
    
    got := parser.NextToken()

    if got.Type != TOK_TEXT {
      t.Errorf("Expected %+v for type but got %+v", TOK_TEXT, got.Type)
    }

    if got.Text != expected[0] {
      t.Errorf("Expected '%+v' for text but got '%+v'", expected[0], got.Text)
    }

    parser.NextToken() //Skip new line
    got = parser.NextToken()

    if got.Type != TOK_TEXT {
      t.Errorf("Expected %+v for type but got %+v", TOK_TEXT, got.Type)
    }

    if got.Text != expected[1] {
      t.Errorf("Expected '%+v' for text but got'%+v'", expected[1], got.Text)
    }

    got = parser.NextToken()

    if got.Type != TOK_EOF {
      t.Errorf("Expected %+v for type but got %+v", TOK_EOF, got.Type)
    }

  })


  t.Run("Can get New Line tokens", func(t *testing.T) {
    markdown := "Hello there\nline2\nline3"
    expectedType := []tokenType { TOK_TEXT, TOK_NEWLINE, TOK_TEXT, TOK_NEWLINE, TOK_TEXT}

    parser := newParser(markdown)

    for i := 0; i < len(expectedType); i++ {
      got := parser.NextToken()

      if got.Type != expectedType[i] {
        t.Errorf("Expected %+v but got %+v index: %d", expectedType, got.Type, i)
      }
    }

    got := parser.NextToken()
    if got.Type != TOK_EOF {
      t.Errorf("Expected %+v but got %+v at end of ast", TOK_EOF, got.Type)
    }
  })

  t.Run("Can parse bullets", func(t *testing.T) {
    cases := []struct {
      markdown string
      types []tokenType
      text []string
      level []int 
    }{
      {
        markdown: ` - test-1
 + test-2
 * test-3`,
      types: []tokenType{ TOK_BULLET, TOK_NEWLINE, TOK_BULLET, TOK_NEWLINE, TOK_BULLET, TOK_EOF},
      text: []string { "test-1", "", "test-2", "", "test-3", "" },
      level: []int{1,0,1,0,1,0},
      },
      {
        markdown: ` - test-1
   + test-2
     * test-3`,
      types: []tokenType{ TOK_BULLET, TOK_NEWLINE, TOK_BULLET, TOK_NEWLINE, TOK_BULLET, TOK_EOF},
      text: []string { "test-1", "", "test-2", "", "test-3", "" },
      level: []int{1,0,2,0,3,0},
      },
    }


    for _, c := range cases {
      parser := newParser(c.markdown)
      for i := 0; i < len(c.types); i++ {
        got := parser.NextToken()

        if c.types[i] == TOK_BULLET {
          if got.Text != c.text[i] {
            t.Errorf("Expected bullet text %+v, got %+v index: %d",c.text[i], got.Text, i)
          }

          if got.Level != c.level[i] {
            t.Errorf("Expected bullet level %+v, got %+v index: %d", c.level[i], got.Level, i)
          }
        }

        if got.Type != c.types[i] {
          t.Errorf("expected %+v, got: %+v index: %d", c.types[i], got.Type, i)
        }
      }
    }
  })

  t.Run("Can parse ordered lists", func(t *testing.T) {
    cases := []struct{
      markdown string
      types []tokenType
      level []int
      text []string
    }{
      {
        markdown: ` 1. Text1
 1. Text2
 1. Text3`,
        types: []tokenType{TOK_ORDEREDITEM, TOK_NEWLINE, TOK_ORDEREDITEM, TOK_NEWLINE, TOK_ORDEREDITEM, TOK_EOF}, 
        level: []int{1,0,1,0,1,0},
        text: []string{ "Text1", "", "Text2", "", "Text3", ""},
      },
      {
         markdown: ` 1. Text1
   1. Text2
     1. Text3`,
        types: []tokenType{TOK_ORDEREDITEM, TOK_NEWLINE, TOK_ORDEREDITEM, TOK_NEWLINE, TOK_ORDEREDITEM, TOK_EOF}, 
        level: []int{1,0,2,0,3,0},
        text: []string{ "Text1", "", "Text2", "", "Text3", ""},     
      },
    }

    for _, c := range cases {
      parser := newParser(c.markdown)

      for i := 0; i < len(c.types); i++ {
        got := parser.NextToken()

        if c.types[i] == TOK_ORDEREDITEM {
          if c.level[i] != got.Level {
            t.Errorf("Expected level %+v got %+v index %d", c.level[i], got.Level, i)
          }

          if c.text[i] != got.Text {
            t.Errorf("Expected text '%+v' got '%+v' index %d", c.text[i], got.Text, i)
          }
        }

        if got.Type != c.types[i] {
          t.Errorf("Expected token type %+v, got %+v index %d", c.types[i], got.Type, i)
        }
      }
    }
  })

  t.Run("Can parse links", func(t *testing.T) {
    
    cases := []struct{
      markdown string
      types []tokenType
      text []string
      linkTypes []linkType
      linkTargets []string
      linkText []string
    }{
      {
        markdown : "[WebLink](http://www.google.com)",
        types : []tokenType{ TOK_LINK },
        text : []string { "0"},
        linkTypes: []linkType { LNK_URL},
        linkTargets : []string{ "http://www.google.com"},
        linkText: []string{ "WebLink"},
      },
      {
        markdown : "[ZKLink](zk:1234)",
        types : []tokenType{ TOK_LINK },
        text : []string { "0"},
        linkTypes: []linkType { LNK_ZK},
        linkTargets : []string{ "1234"},
        linkText: []string{ "ZKLink"},
      },
      {
        markdown : "[ZKALink](zka:1234)",
        types : []tokenType{ TOK_LINK },
        text : []string { "0"},
        linkTypes: []linkType { LNK_ZKA},
        linkTargets : []string{ "1234"},
        linkText: []string{ "ZKALink"},
      },
      {
        markdown : "[ReportLink](rp:1234)",
        types : []tokenType{ TOK_LINK },
        text : []string { "0"},
        linkTypes: []linkType { LNK_REPORT},
        linkTargets : []string{ "1234"},
        linkText: []string{ "ReportLink"},
      },
      {
        markdown : "[EmptyLink]()",
        types : []tokenType{ TOK_LINK },
        text : []string { "0"},
        linkTypes: []linkType { LNK_EMPTY},
        linkTargets : []string{ ""},
        linkText: []string{ "EmptyLink"},
      },
      {
        markdown : "[EmptyLink]( )",
        types : []tokenType{ TOK_LINK },
        text : []string { "0"},
        linkTypes: []linkType { LNK_EMPTY},
        linkTargets : []string{ ""},
        linkText: []string{ "EmptyLink"},
      },
      {
        markdown : "[WebLink](http://www.google.com)[AnotherLink](zk:1234)",
        types : []tokenType{ TOK_LINK, TOK_LINK },
        text : []string { "0", "1"},
        linkTypes: []linkType { LNK_URL, LNK_ZK},
        linkTargets : []string{ "http://www.google.com", "1234"},
        linkText: []string{ "WebLink", "AnotherLink"},
      },
    }

    for _, c := range cases {
      parser := newParser(c.markdown)

      for i := 0; i < len(c.types); i++ {
        got := parser.NextToken()
        lnks := parser.Links()

        if c.types[i] == TOK_LINK {
          if got.Text != c.text[i] {
            t.Errorf("Expected link name to be in text field of token %+v, got %+v index %d", c.text[i], got.Text, i)
          }

          if lnks[i].Type != c.linkTypes[i] {
            t.Errorf("Expected link type %+v, got %+v, index %d", c.linkTypes[i], lnks[i].Type, i) 
          }

          if lnks[i].Title != c.linkText[i] {
            t.Errorf("Expected link title %+v, got %+v index %d", c.linkText[i], lnks[i].Title, i)
          }
        }
        
        if got.Type != c.types[i] {
          t.Errorf("Expected Type %+v got %+v", c.types[i], got.Type)
        }
      }
    }
  })
}

