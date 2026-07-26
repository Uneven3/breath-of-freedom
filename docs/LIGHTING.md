# Iluminación y atmósfera

Existe porque `NORTE.md` declara que la belleza de este juego es **luz + color +
atmósfera**, no detalle geométrico — y es el único sistema visual de esa frase
sin reglas escritas. Hoy sus ~37 decisiones (paleta por hora, `SHADOW_CASTING_LUX`,
sombras 2048→1024, ambiente nocturno, `DistanceFog` 45→240 m) viven dispersas en
`world/day_night.rs`, `camera/mod.rs` y `perf/`, y ya hay trampas que nadie
adivina: **cambiar las cascadas en vivo panica la contabilidad de visibilidad de
Bevy**, por eso se fijan al arrancar con `BOF_CASCADES`.

Vacío a propósito: se llena midiendo, con el molde de `BOTWGrass.md`.
