# Current example diagrams

These TOML files use FLEECE's current diagram-only schema. They contain no
simulation values, register storage or retired configuration fields. All use
Original dark, a visible grid and grid snapping. The PNG/SVG previews are exported
from these same documents using the current renderer.

| Document | Demonstrates | Previews |
| --- | --- | --- |
| [half-adder.toml](half-adder.toml) | Two inputs, XOR/AND gates, fan-out and routed wires; also available through File → Half-adder example | [PNG](half-adder.png), [SVG](half-adder.svg) |
| [state-register.toml](state-register.toml) | A two-bit D-flip-flop with explicit CLK/RST connections, three-input gates, colored feedback and math labels | [PNG](state-register.png), [SVG](state-register.svg) |
| [fsm.toml](fsm.toml) | Two states, initial/accepting markers, reciprocal transitions, a self-loop and annotation-only conditions/outputs | [PNG](fsm.png), [SVG](fsm.svg) |

Open a document from File → Open or pass its path to the desktop app:

```sh
./target/debug/fleece examples/half-adder.toml
```

In the browser, use File → Open or drop a `.toml` file onto the canvas. The image
previews are for viewing; open the TOML document to continue editing.

Regenerate previews after changing a document or the export renderer:

```sh
for name in half-adder state-register fsm; do
    ./target/debug/fleece "examples/$name.toml" -o "examples/$name.png"
    ./target/debug/fleece "examples/$name.toml" -o "examples/$name.svg"
done
```

Exports are independent of the saved editor theme and viewport. Conditions,
module widths and circuit equations are diagram annotations, not simulation.
