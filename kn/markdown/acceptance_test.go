package markdown

import (
	"reflect"
	"testing"
)

func TestMarkdown(t *testing.T) {
 t.Run("Markdown outputs headings", func(t *testing.T) {

    markdown := `# Heading #1
## Heading #2
### Heading #3
#### Heading #4
##### Heading #5
###### Heading #6`
    
    expected := `[blue::b] Heading #1[-:-:-]
[blue::b] Heading #2[-:-:-]
[blue::b] Heading #3[-:-:-]
[blue::b] Heading #4[-:-:-]
[blue::b] Heading #5[-:-:-]
[blue::b] Heading #6[-:-:-]`


  got, _ := MarkdownToTui(markdown)

  if got != expected {
    t.Errorf("Expected '%s', got: '%s'", expected, got)
  }
  })

  t.Run("Markdown unordered lists output properly", func(t *testing.T) {
    markdown := ` - level 1 bullet
 + another one
   - level 2 bullet
   + level 2 bullet again
 * another level 1
   * another level 2
     - level 3
     + another level 3
     * yet another level 3`

    expected := ` [green]ﱣ[-] level 1 bullet
 [green]ﱣ[-] another one
   [green]ﱤ[-] level 2 bullet
   [green]ﱤ[-] level 2 bullet again
 [green]ﱣ[-] another level 1
   [green]ﱤ[-] another level 2
     [green][-] level 3
     [green][-] another level 3
     [green][-] yet another level 3`

    got, _ := MarkdownToTui(markdown)

    if got != expected {
      t.Errorf("Expected '%s', got '%s'", expected, got)
    }
  })

  t.Run("markdown ordered lists outputs properly", func(t *testing.T) {
    markdown := ` 1. Fooo
 1. Bar
 1. Bar
   1. Foobar
   1. Foobar
 1. Woo
   1. Bar
     1. Bar
     1. Foo`

    expected := ` [green]01)[-] Fooo
 [green]02)[-] Bar
 [green]03)[-] Bar
 [green]03.01)[-] Foobar
 [green]03.02)[-] Foobar
 [green]04)[-] Woo
 [green]04.01)[-] Bar
 [green]04.01.01)[-] Bar
 [green]04.01.02)[-] Foo`

    got, _ := MarkdownToTui(markdown)

    if got != expected {
      t.Errorf("Expected '%s', got '%s'", expected, got)
    }

  })

  t.Run("Can render links in markdown", func(t *testing.T) {
    markdown := `[HTTP Link](http://www.google.com) [ZK Link](zk:123) [ZK Attach](zka:123) [Report](rp:foo) [Empty]() [Empty With Space]( )`
    expected := `["0"][blue::u]HTTP Link[-:-:-][""] ["1"][blue::u]ZK Link[-:-:-][""] ["2"][blue::u]ZK Attach[-:-:-][""] ["3"][blue::u]Report[-:-:-][""] ["4"][blue::u]Empty[-:-:-][""] ["5"][blue::u]Empty With Space[-:-:-][""]`
    expectedLinks := []link{
      {
        Type: LNK_URL,
        Target: "http://www.google.com",
        Index: 0,
        Title: "HTTP Link",
      },
      {
        Type: LNK_ZK,
        Target: "123",
        Index: 1,
        Title: "ZK Link",
      },
      {
        Type: LNK_ZKA,
        Target: "123",
        Index: 2,
        Title: "ZK Attach",
      },
      {
        Type: LNK_REPORT,
        Target: "foo",
        Index: 3,
        Title: "Report",
      },
      {
        Type: LNK_EMPTY,
        Target: "",
        Index: 4,
        Title: "Empty",
      },

      {
        Type: LNK_EMPTY,
        Target: "",
        Index: 5,
        Title: "Empty With Space",
      },
    }

    gotMark, gotLinks := MarkdownToTui(markdown)

    if gotMark != expected{
      t.Errorf("Expected '%s', got '%s'", expected, gotMark)
    }

    for i, l := range expectedLinks {

      if !reflect.DeepEqual(l, gotLinks[i]) {
        t.Errorf("Expected %+v, got %+v", l, gotLinks[i])
      }
    }
  })

/*  t.Run("Can render code blocks and inline code items", func(t *testing.T) {
    markdown := "Hey `codeage`\n" +
    "```\n" +
    "more code\n" +
    "```\n" +
    "\n" +
    "````\n" +
    "```\n" +
    "````\n" +
    "\n" +
    "```go\n" +
    "code\n" +
    "```"

    expected := "Hey [green]codeage[-:-:-]\n" +
    "[green:gray]" +
    "more code\n" +
    "[-:-:-]" +
    "\n" +
    "[green:gray]" +
    "```\n" +
    "[-:-:-]" +
    "\n" +
    "[green:gray]" + 
    "code\n" + 
    "[-:-:-]"

    got, _ := MarkdownToTui(markdown)

    if got != expected {
      t.Errorf("Expected '%+v', got '%+v'", expected, got)
    }

  })*/
}
