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


}
