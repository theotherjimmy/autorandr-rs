autorandrd(1)

# NAME

autorandrd - An automatic display configuration daemon for X11


# SYNOPSIS

*autorandrd* _--help_++
*autorandrd* [*-c*,*--config*] _CONFIG_


# DESCRIPTION

*autorandrd* listens to monitor changes and reconfigures the attached
monitors to mach a configuration that is specific to the monitors plugged in.

The first mode prints a summary of the options.
In the second mode, *autorandrd* will run as a daemon in the foreground.
First, *autorandrd* reads the config file and checks for errors.
At startup and after any attached monitor updates, *autorandrd* updates the
monitor config to a matching configuration in the configuration file.



# OPTIONS

_CONFIG_
	Use this configuration file. See *autorandrd*(5) for details on the
	format.

*-c*, *--config*
	Instead of daemonizing, check the config specified and exit.

*-h*, *--help*
	print usage info and exit.


# EXIT STATUS

*0*	When check mode is specified, *autorandrd* found no errors.

*1*	*autorandrd* could not find the config file or found syntax errors.


# SEE ALSO
*autorandrd*(5)


# AUTHORS
Maintained by Jimmy Brisson. Up to date sources can be found at
https://github.com/theotherjimmy/autorandr-rs and bugs/patchs may be submitted
through the builtin github issue tracker.
