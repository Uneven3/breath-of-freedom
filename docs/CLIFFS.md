# Acantilados: el terreno hace la planta, la roca hace la pared

Cómo se construye en este juego una pared escalable, y por qué no se construye
esculpiendo el terreno. Vivo: se corrige cuando la técnica cambie.

## De dónde sale la regla

Un heightmap es `y = f(x,z)`: una altura por celda, así que **no puede plegarse
sobre sí misma**. No hay cueva, no hay saliente y no hay vertical a ninguna
resolución — lo que parece pared es siempre una rampa del ancho de una celda.
Por eso el terreno dejó de ser escalable el 2026-08-23 y nace `NonClimbable`.

Pero el heightmap no se abandona, porque compra cuatro cosas que una malla no
da: pesa una lista de floats, se simplifica con la distancia tomando uno de cada
N puntos, contesta *"¿qué altura hay acá?"* mirando una casilla en vez de
recorrer un árbol de triángulos, y es una grilla donde también se anota qué
material tiene cada celda. Lo que paga por eso son tres límites, y los tres
empujan al mismo lugar:

- **Geometría**: no hay verticales.
- **Textura**: el UV del terreno se proyecta desde arriba (`u = x/12`, `v =
  z/12`), así que cuanto más se empina una cara, más se estira la imagen. En una
  vertical el estiramiento es infinito. El paliativo clásico —*triplanar*, tres
  muestras mezcladas por la normal— cuesta el triple y no arregla la silueta.
- **Resolución uniforme**: el detalle de una grieta de 10 cm exige puntos cada
  10 cm **en los 320 m del mundo**.

De ahí la práctica de la industria, que es la que sigue este proyecto: **el
heightmap hace las formas grandes y el suelo caminable; el detalle, la silueta y
lo escalable son mallas colocadas encima**, que además se instancian y se
descartan una por una cuando no se ven.

## La pieza va enterrada, no apoyada

Sobre el llano, el acantilado deja 12 m de pared y entierra 3; sobre el relieve
de Terreno, que ya estaba a 4 o 5 m, deja unos 8 y entierra el resto. Tres
razones, y ninguna es estética:

1. **Una intersección limpia se ve.** Dos superficies que se tocan tangencial-
   mente dejan una línea de contacto continua con dos sombreados distintos a los
   lados, y el ojo la lee como "objeto apoyado" antes de poder decir por qué.
   Enterrada, esa línea queda dentro del volumen de la roca.
2. **El suelo se mueve.** El terreno se simplifica con la distancia y lleva
   margen de colisión; cualquiera de las dos cosas hace flotar o hundir una
   pieza apoyada. Con parte del cuerpo bajo tierra, esos centímetros no existen.
3. **Sobrevive a que se esculpa al lado.** Lo apoyado hay que reasentarlo.

## El contacto se rompe con escombro

Aun enterrada, la curva por donde la roca entra en el suelo es demasiado
prolija. La solución no es recortar mejor: es **poner algo sobre el borde**
(`world::debris`). Y en la naturaleza ese algo ya está — al pie de un acantilado
hay derrubio, porque la roca se fractura y cae.

Las piedras siguen la **huella real** de la pieza, que es una elipse: con
semiejes de 9 y 3 m, un anillo de radio fijo dejaría piedras flotando de un lado
y enterradas del otro. Y van con el ángulo, la distancia y el tamaño
desordenados: equiespaciadas sobre la elipse exacta se leen como una guarda de
jardín, que es justo la costura que vienen a esconder.

**Sin collider, y es decisión.** Darle cuerpo a una piedra de 15 cm cuesta dos
defectos medidos por un adorno: a 0,35 m de alto entra en la ventana de vault
(`vault_min_height` = 0,3) y le come el input al jugador que viene a escalar; y
una esfera rozando la cápsula produce contactos por debajo del límite de piso,
que es el zumbido `Walk`↔`Fall` reintroducido en el punto exacto donde uno se
para a escalar. Se atraviesan, y a esa altura no se nota.

## Una pieza, no cuatro solapadas

Un acantilado de catálogo se arma solapando rocas. Acá no, y la razón es del
motor: **cada costura entre dos elipsoides es un salto de la normal de la cara**
—medido entre 32° y 64°— y `motors::climb` filtra la normal con
`NORMAL_SMOOTHING_TAU = 0,08`, calibrado contra una perturbación de **7°**.
Cruzar una costura escalando sacudiría el cuerpo durante ~0,24 s, con
`up_along_face` apuntando a la cara vieja mientras el pegado empuja a la nueva.
Es uno de los dos defectos que ya se reportaron jugando; no hay que fabricar
tres más a propósito.

La variedad de silueta la dan `seed` y `bump_metres`. Y una pieza sola es
también **un draw en vez de cuatro**, que importa porque el techo móvil son 100.

Si algún día se quieren varias masas, la salida no es solapar: es **separarlas**
—`no_two_crags_overlap` lo exige y por buenas razones— o subir el filtro de
normal y medirlo antes de fijar la geometría.

## Lo que todavía no está

- **Aplanar la huella al colocar** (*landscape blending* barato): que el editor
  emparejе el terreno bajo un prop con `flatten_area`, como se asienta un
  ladrillo en arena. Es acción de **autoría**, no de arranque: un sistema que
  esculpa al entrar a la escena pisaría lo autorado y se persistiría con Ctrl+S,
  contra la regla de que el archivo *es* el nivel.
- **Colocar en vez de declarar.** Las instancias del editor no tienen collider
  (`visuals/instances.rs` es sólo visual, y §20 impide que presentación los
  arme), así que el acantilado vive en una tabla de `world::crags`.
- **Mezclar el material de la roca con el del suelo** cerca del contacto, que es
  lo que los motores grandes hacen con una virtual texture. Hoy lo tapa el
  escombro y nada más.
