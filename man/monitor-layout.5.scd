monitor-layout(5)

# NAME

*monitor-layout.kdl* - configuration file for *monitor-layout*


# SYNTAX

The *monitor-layout.kdl* file is in KDL format.
See https://kdl.dev for more information.


# SUPPORTED NODES

*monitor*
	This node specifies a single monitor.
	This node accepts a required, positional parameter, the monitor's _alias_, 
	and two optional properties, the _product_ name and the _serial_ number.

	The _product_ matches the product name of the monitor.
	For example, the author owns an Acer K272HUL monitor which reports its
	product as "Acer K272HUL" and a Samsung S27D850 that reports "S27D850".
	This field is treated as an opaque string that matches a parsed EDID.

	The _serial_ matches the serial number of the monitor.
	For example, the author owns an Acer K272HUL monitor which reports its
	product serial as "T0SAA0014200".
	This feild is treated as an opaque string that matches a parsed EDID.

	For a tool that dumps this information, see *randr-edid*(1)

*layout*
	This node specifies a layout of monitors that should be automatically
	configured.
	The _layout_ node accepts a single positional parameter, it's name, and
	a series of children nodes that may either be _matches_ or _monitor_

*layout.matches*
	This node specifies which monitors, by _alias_, must be connected to
	apply this layout.
	It accepts the list of monitor _aliases_ as a sequence of postional
	parameters.

*layout.monitor*
	This node specifies the geometry of a single monitor.
	*layout.monitor* accepts an _alias_ as its only positional parameter,
	and 5 properties, _w_, _h_, _x_, _y_ and _primary_.
	All properties are mandatory, except for _primary_, which defaults to
	false when not present.
	The _w_ and _h_ specifiy the width and height of the mode to select for
	this monitor respectively.
	The _x_ and _y_ specifiy the offset from the 0,0 coodinate.
	_primary_ specifies that this monitor should become the primary monitor
	when this layout is enabled.
	It is unspecified what happens when multiple *layout.montor* nodes within
	the same *layout* specify _prymary_ as true.


# SEE ALSO
*monitor-layout*(1)

# AUTHORS
Maintained by Jimmy Brisson. Up to date sources can be found at
https://github.com/theotherjimmy/autorandr-rs and bugs/patchs may be submitted
through the builtin github issue tracker.
