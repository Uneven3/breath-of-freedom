# System Model & State

## Single Sources of Truth (SSoT)
| State / Data | Owning Module / Entity | Access Pattern |
|--------------|------------------------|----------------|
| [State]      | [Owner]                | [e.g., Read/Write vs Read-Only] |

## Core Entities / Nodes
*Use the DSL relevant to the tech stack (e.g., Class, Node, Table, Struct)*

### `[EntityName]`
- **Responsibility:** [Single responsibility]
- **State Held:** [Data it stores]
- **Invariants:** [Rules that can never be broken (e.g., Balance >= 0)]

### `[EntityName]`
...