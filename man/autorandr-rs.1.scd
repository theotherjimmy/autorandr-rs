autorandr-rs(1)

# NAME

autorandr-rs - An automatic display configuration daemon for X11


# SYNOPSIS

*autorandr-rs* _--help_++
*autorandr-rs* [*-c*,*--config*] _CONFIG_


# DESCRIPTION

*autorandr-rs* listens to monitor changes and reconfigures the attached
monitors to mach a configuration that is specific to the monitors plugged in.

The first mode prints a summary of the options.
In the second mode, *autorandr-rs* will run as a daemon in the foreground.
First, *autorandr-rs* reads the config file and checks for errors.
At startup and after any attached monitor updates, *autorandr-rs* updates the
monitor config to a matching configuration in the configuration file.



# OPTIONS

_CONFIG_
	Use this configuration file. See *autorandr-rs*(5) for details on the
	format.

*-c*, *--config*
	Instead of daemonizing, check the config specified and exit.

*-h*, *--help*
	print usage info and exit.


# EXIT STATUS

*0*	When check mode is specified, *autorandr-rs* found no errors.

*1*	*autorandr-rs* could not find the config file specified.

*2*	*autorandr-rs* found syntax errors within the config file.


# SEE ALSO
*autorandr-rs*(5)


# AUTHORS
Maintained by Jimmy Brisson. Up to date sources can be found at
https://github.com/theotherjimmy/autorandr-rs and bugs/patchs may be submitted
through the builtin github issue tracker.
