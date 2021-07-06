package markdown

import "testing"

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


  got := MarkdownToTui(markdown)

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

    got := MarkdownToTui(markdown)

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

    got := MarkdownToTui(markdown)

    if got != expected {
      t.Errorf("Expected '%s', got '%s'", expected, got)
    }

  })
}
