autorandr-rs(5)

# NAME

*autorandr-rs.toml* - configuration file for *autorandr-rs*


# SYNTAX

The *autorandr-rs.toml* file is in TOML format.
See https://toml.io for more information.


# SUPPORTED KEYS

*monitors*
	This section specifies a mapping from _aliases_ to more detailed
	monitor information.

*monitors.*_<alias>_
	Each subsection of the *monitors* section creates an alias for a monitor
	and describes how to match that _alias_ based on it's EDID.
	The name of the monitor may not be "_monitors_", as that conflicts with
	another key in the *configurations.*_<name>_ section.

*monitors.*_<alias>_*.product*
	This matches the product name of the monitor.
	For example, the author owns an Acer K272HUL monitor which reports its
	product as "Acer K272HUL" and a Samsung S27D850 that reports "S27D850".
	This field is treated as an opaque string that matches a parsed EDID.
	This field is required.
	At the time of writing the author cannot recomend a tool to dump this
	information.

*monitors.*_<alias>_*.serial*
	This matches the serial "number" of the monitor.
	For example, the author owns an Acer K272HUL monitor which reports its
	product serial as "T0SAA0014200".
	This feild is treated as an opaque string that matches a parsed EDID.
	This field is required.
	At the time of writing the author cannot recomend a tool to dump this
	information.

*configurations*
	This section specifies an unordered set of configurations.

*configurations.*_<name>_
	This section specifies a single, named configuraiton and set of _aliases_
	that, when attached, trigger *autorandr-rs* to apply this configuration.
	The _name_ has no restrictions.

*configurations.*_<name>_*.monitors*
	This key specifies a set of _aliases_ to match as an array.
	When the monitors in the set of _aliases_ are attached, this configuration
	is applied to the X11 server.
	If two configurations have the same set of monitor _aliases_, it is
	unspecified which configuration _name_ will be applied at runtime.
	This field is required.

*configurations.*_<name>_*.*_<alias>_
	This section configures the monitor matching _alias_ as part of the
	configuration _name_.

*configurations.*_<name>_*.*_<alias>_*.mode*
	Specify the mode of the attached monitor, _alias_.
	This is formatted as *<width>*x*<height>*.
	*width* and *height* are limited to the range available to a _u16_.
	This field is required.

*configurations.*_<name>_*.*_<alias>_*.position*
	Specify the absolute position of the attached monitor, _alias_.
	This is formatted as *<x>*x*<y>*.
	*x* and *y* are limited to the range available to an _i16_.
	This field is required.

*configurations.*_<name>_*.*_<alias>_*.primary*
	Specifiy that the monitor _alias_ is the primary disply.
	This field is optional and defaults to _false_.

# SEE ALSO
*autorandr-rs*(1)

# AUTHORS
Maintained by Jimmy Brisson. Up to date sources can be found at
https://github.com/theotherjimmy/autorandr-rs and bugs/patchs may be submitted
through the builtin github issue tracker.
