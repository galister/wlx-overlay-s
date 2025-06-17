# Universal widget attributes

`display`: flex | block | grid

`position`: absolute | relative

`flex_grow`: units (3)

`flex_shrink`: units (3)

`gap`: units (42) or percent (42%)

`flex_basis`: units (42) or percent (42%)

`justify_self`: center | end | flex_end | flex_start | start | stretch

`justify_content`: center | end | flex_start | flex_end | space_around | space_between | space_evenly | start | stretch

`flex_wrap`: wrap | no_wrap | wrap_reverse

`flex_direction`: row | column | column_reverse | row_reverse,

`align_items`, `align_self`: baseline | center | end | flex_start | flex_end | start | stretch

`box_sizing`: border_box | content_box

`margin`, `margin_left`, `margin_right`, `margin_top`, `margin_bottom`: units (42) or percent (42%)

`padding`, `padding_left`, `padding_right`, `padding_top`, `padding_bottom`: units (42) or percent (42%)

`overflow_x`, `overflow_y`: hidden | visible | clip | scroll

`min_width`, `min_height`: units (42) or percent (42%)

`max_width`, `max_height`: units (42) or percent (42%)

`width`, `height`: units (42) or percent (42%)

# Widgets

### `div`

The most simple element

#### Parameters

None

---

### `label`

Text element

#### Parameters

`text`: abc

`color`: #FFAABB | #FFAABBCC

`align`: left | right | center | justified | end

`weight`: normal | bold

`size`: _float_

---

### `rectangle`

A styled rectangle

#### Parameters

`text`: abc

`color`: #FFAABB | #FFAABBCC

_1st gradient color_

`color2`: #FFAABB | #FFAABBCC

_2nd gradient color_

`gradient`: horizontal | vertical | radial | none

`round`: _float (0.0 - 1.0)_

`border`: _float_

`border_color`: #FFAABB | #FFAABBCC

---

### `sprite`

Image widget, supports raster and svg vector

#### Parameters

`src`: Internal (assets) image path

`src_ext`: External image path

---
