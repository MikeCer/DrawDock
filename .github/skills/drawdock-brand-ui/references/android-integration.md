# Android integration

## Icons

The VectorDrawables are white. Tint them where you use them.

Compose:
```kotlin
Icon(painterResource(R.drawable.ic_dd_shapes), contentDescription = "Shapes",
     tint = if (selected) DDColor.menuItemSelectedIcon else DDColor.onDark,
     modifier = Modifier.size(DDDimen.menuIcon))
```
Custom `View`/`Canvas` (common for radial menus):
```kotlin
val d = AppCompatResources.getDrawable(ctx, R.drawable.ic_dd_shapes)!!.mutate()
d.setTint(ContextCompat.getColor(ctx, R.color.dd_on_dark))
d.setBounds(l, t, l + iconPx, t + iconPx); d.alpha = if (enabled) 255 else (0.38f * 255).toInt(); d.draw(canvas)
```
Cache the mutated drawables (or `ImageBitmap`s) per state. Don't inflate them inside `onDraw`.

## Tokens

- Views: copy `dd_colors.xml` and `dd_dimens.xml` to `res/values/`. Read them with `ContextCompat.getColor` and `resources.getDimension`. Motion values are `<integer name="dd_motion_*">`.
- Compose: copy `DrawDockTokens.kt` and set its package. `DDColor.inkPalette` gives the 7 swatches in menu order.
- If the PoC already defines ink colors in its model layer (e.g. an enum sent to the PC), keep those values as the source of truth. Map them to swatches; don't fork them.

## Launcher icon (only when the user asks to rebrand)

- Copy `res/mipmap-*/ic_launcher.png`, `ic_launcher_foreground.png`, `res/mipmap-anydpi-v26/ic_launcher.xml` and `res/values/ic_launcher_background.xml` (color `#F0F7FF`).
- If the app uses `ic_launcher_round`, point it at the same adaptive XML. Remove stale `ic_launcher.webp` duplicates so the resources don't clash.
- The Play Store icon is `assets/app-icon/drawdock-play-store-512-fullbleed.png`.

## Splash

- Android 12+ system splash: add `androidx.core:core-splashscreen`, merge `res/values/dd_splash_theme.xml`, copy `res/drawable-xxxhdpi/dd_splash_icon.png`, set `postSplashScreenTheme` to the real app theme and call `installSplashScreen()` in the launcher activity before `super.onCreate`.
- Illustrated splash (optional): Android 12+ can't show full-bleed images in the system splash. Show one of `assets/splash/splash-*.png` on a short branded intro/loading screen (while the connection initializes). Use `ContentScale.Crop` / `centerCrop` on `brand_navy`, with `assets/logo/drawdock-logo-horizontal-dark-bg.png` in the top third (the images keep it clear).
- `splash-03-radial-menu.png` shows the radial menu and is the best default. Copy at most 1–3 images into `res/drawable-nodpi/` as WebP (quality 85) to keep the APK small.
