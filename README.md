# wl-crosshair
A crosshair overlay for wlroots compositors.

A extremely stripped down version of [crossover](https://github.com/lacymorrow/crossover).

Currently has no support for command line arguments or any customization.

### Preview:
![image](https://github.com/lelgenio/wl-crosshair/assets/31388299/6e0aaa16-837b-40a8-9a13-ed808ea5db86)

### Why is it flickering when I put my cursor over it?
In wayland, windows cannot be "click-through", so in order to still send events we "close" the window when you hover it and show it in the next frame.
