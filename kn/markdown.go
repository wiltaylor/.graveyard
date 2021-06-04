package main

import (
	"fmt"
	"strings"
)

type MkNode interface {
	GetText() string
}

type MkText struct {
	Text      string
	Bold      bool
	Underline bool
	Code      bool
}

type MkHeading struct {
	Text  string
	Level int
}

type MkTable struct {
	Columns      int
	Rows         []MkTableRow
	ColumnWidths []int
}

type MkTableRow struct {
	Cells []string
}

type MkLink struct {
	Title string
	Url   string
	Id    string
	Icon  string
}

type MkBullet struct {
	Level int
	Text  string
}

type MkNumberBullet struct {
	Level   int
	Text    string
	Ordinal int
}

type MkCodeFence struct {
	Text     string
	Language string
}

type MkImage struct {
	Title string
	Url   string
	Id    string
}

type MarkdownParser struct {
  lines []string
  row int
  col int
}

func (h *MarkdownParser)peek(chars int) string {
  if chars > len(h.lines[h.row]) - h.col {
    return ""
  }

  return string(h.lines[h.row][h.col:h.col + chars])
}

func (h *MarkdownParser)peekString() (string, bool) {
  result := ""
  line := h.lines[h.row]

  for i := h.col; i < len(line); i++ {
    h.col = i

    if h.peek(2) == "![" {
      return result, false
    }

    if h.peek(1) == "[" || h.peek(1) == "*" || h.peek(1) == "_" {
      return result, false
    }

    result += h.peek(1)
  }

  h.row++
  h.col = 0

  return result, true
}

func (h *MarkdownParser)peekTill(text string) (string, bool) {
  result := ""
  line := h.lines[h.row]

  for i := h.col; i < len(line) - len(text); i++ {
    h.col = i
    if string(line[i:i+len(text)]) == text {
      return result, false
    }

    result += string(line[i])
  }

  return result, true
}

func NewMarkdownParser(text string) MarkdownParser {
  result := MarkdownParser{lines: make([]string, 0), row: 0, col: 0}

  line := ""
  for _, c := range text {
    if c == '\n' {
      result.lines = append(result.lines, line)
      line = ""
      continue
    }

    line += string(c)
  }

  return result
}


func (h *MarkdownParser) Next() (MkNode, bool) {
  if h.row >= len(h.lines) {
    return nil, true
  }

  if h.col == 0 {
    if h.lines[h.row] == "" {
      txt := MkText{Text:"", Bold: false, Code: false, Underline: false}
      h.row++
      h.col = 0
      return &txt, true
    }

    if h.peek(1) == "#" {
      result := MkHeading{Text: "", Level: 1}

      if h.peek(6) == "######" {
        result.Level = 6
      } else if h.peek(5) == "#####" {
        result.Level = 5
      } else if h.peek(4) == "####" {
        result.Level = 4
      } else if h.peek(3) == "###" {
        result.Level = 3
      } else if h.peek(2) == "##" {
        result.Level = 2
      } else {
        result.Level = 1
      }

      txt := h.lines[h.row][result.Level:]
      result.Text = txt
      h.row++
      h.col = 0
      return &result, true
    }

    if h.peek(3) == " - " || h.peek(3) == " * " || h.peek(3) == " + " {
      h.col += 3
      result := MkBullet{Level: 1, Text: ""}

      txt, nl := h.peekString()

      result.Text = txt
      nl = true

      return &result, nl
    }

    if h.peek(5) == "   - " || h.peek(5) == "   * " || h.peek(5) == "   + " {
      h.col += 5
      result := MkBullet{Level: 2, Text: ""}

      txt, nl := h.peekString()

      result.Text = txt
      nl = true

      return &result, nl
    }

    if h.peek(7) == "     - " || h.peek(7) == "     * " || h.peek(7) == "     + " {
      h.col += 7
      result := MkBullet{Level: 3, Text: ""}

      txt, nl := h.peekString()

      result.Text = txt
      nl = true

      return &result, nl
    }

    if h.peek(1) == "[" {
      h.col++
      title, nl := h.peekTill("]")

      if nl {
        txtResult := MkText{Text: "[" + title, Bold: false, Code: false, Underline: false}
        return &txtResult, true
      }

      if h.peek(1) == "(" {
        h.col++

        url, nl := h.peekTill(")")

        if url == "" {
          
        }

        if nl {
          
        }

      }

    }

    txtResult := MkText{Text: h.lines[h.row], Bold: false, Underline: false, Code: false}
    h.row++
    h.col = 0

    return &txtResult, true
  }

  remainingText, _ := h.peekString()
  remainingResult := MkText{Text: remainingText, Bold :false, Underline: false, Code: false}
  h.row++
  h.col = 0

  return &remainingResult, true
}

func (h *MkText) GetText() string {

	fmtString := ""
	fmtColour := "-"

	if h.Bold {
		fmtString += "b"
	}

	if h.Underline {
		fmtString += "u"
	}

	if h.Code {
		fmtColour = "green"
	}

	return fmt.Sprintf("[%s:-:%s]%s[-:-:-]", fmtColour, fmtString, h.Text)
}

func (h *MkHeading) GetText() string {
	switch h.Level {
	case 1:
		return fmt.Sprintf("[red::b] %s[-:-:-]", h.Text)
	case 2:
		return fmt.Sprintf(" [blue::b] %s[-:-:-]", h.Text)
	case 3:
		return fmt.Sprintf("  [green::b] %s[-:-:-]", h.Text)
	case 4:
		return fmt.Sprintf("   [green::b] %s[-:-:-]", h.Text)
	case 5:
		return fmt.Sprintf("    [green::b] %s[-:-:-]", h.Text)
	case 6:
		return fmt.Sprintf("     [green::b] %s[-:-:-]", h.Text)
	default:
		return fmt.Sprintf("[::b] %s[-:-:-]", h.Text)
	}
}

func padString(text string, length int) string {
	return text + strings.Repeat(" ", length-len(text))
}

func (h *MkTable) GetText() string {
	result := ""

	for i, row := range h.Rows {
		result += "|"

		for c := 0; c < h.Columns; c++ {
			result += padString(row.Cells[c], h.ColumnWidths[i]) + "|"
		}

		result += "\n"

		if i == 0 {
			result += "|"
			for c := 0; c < h.Columns; c++ {
				result += strings.Repeat("-", h.ColumnWidths[i]) + "|"
			}
			result += "\n"
		}
	}

	return result
}

func (h *MkLink) GetText() string {
	return fmt.Sprintf("[\"%v\"]%s[blue::u]%s[-:-:-][\"\"]", h.Id, h.Icon, h.Title)
}

func (h *MkBullet) GetText() string {
	switch h.Level {
	case 1:
		return fmt.Sprintf(" [green]ﱣ[-] %s", h.Text)
	case 2:
		return fmt.Sprintf("   [green]ﱤ[-] %s", h.Text)
	case 3:
		return fmt.Sprintf("     [green][-] %s", h.Text)
	default:
		return h.Text
	}
}

func (h *MkNumberBullet) GetText() string {
	switch h.Level {
	case 1:
		return fmt.Sprintf(" [green]%v.[-] %s", h.Ordinal, h.Text)
	case 2:
		return fmt.Sprintf("   [green]%v.[-] %s", h.Ordinal, h.Text)
	case 3:
		return fmt.Sprintf("     [green]%v.[-] %s", h.Ordinal, h.Text)
	default:
		return h.Text
	}
}

func (h *MkCodeFence) GetText() string {
	return fmt.Sprintf("[green:gray]%s[-:-:-]", h.Text)
}

func (h *MkImage) GetText() string {
	return fmt.Sprintf("[blue::u]IMG:%s[-:-:-]", h.Title)
}
