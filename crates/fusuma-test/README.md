# fusuma-test

Drives an Elm program (`fusuma::Program`) the way a person would, headless, on Vello's CPU renderer:

- **find** a widget by the text it shows, the name the app gave it (`Id`), or its role;
- **click** it (pointer events at its centre) or **activate** it (focus and Enter, the way a keyboard or an accessibility
  tool does), type, press keys, hover, let time pass;
- read the program's **state**, the **messages** it handled (with the time of each) and what is **on the screen**;
- take a **screenshot** (PNG) and compare it with another;
- run a **script** (one command a line: `click Save`, `type abc`, `wait 500`, `expect Saved`) and get one back out of a
  session (`recorded`).

It uses the semantic tree, so a widget a test cannot find is a widget an assistive tool cannot find either.
