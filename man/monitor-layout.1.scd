autorandrd(1)

# NAME

autorandrd - An automatic display configuration daemon for X11


# SYNOPSIS

*monitor-layout* [*-v* | *--verbose*] *print-edids*++
*monitor-layout* [*-v* | *--verbose*] *check* _CONFIG_++
*monitor-layout* [*-v* | *--verbose*] *daemon* _CONFIG_


# DESCRIPTION

*monitor-layout* is a group of related tools to automate monitor layout changes
when the attached monitors change.


# OPTIONS

_CONFIG_
	Use this configuration file. See *monitor-layout*(5) for details on the
	format.

*-v*, *--verbose*
	Be more verbose, showing more information on stderr each time it's specified.

*-h*, *--help*
	print usage info and exit.


# COMMANDS

*print-edids*
	Print the edids of all attached monitors in a format compatible with the *daemon*
	command, using the port the monitor in place of the name.

*check*
	Check that the configuration file contains no errors.

*daemon*
	In the foreground, run a daemon that waits for monitor connection and disconnection
	events and applies the layouts specified in _CONFIG_ when the attached monitors
	matches a specifed layout.


# SEE ALSO
*monitor-layout*(5)


# AUTHORS
Maintained by Jimmy Brisson. Up to date sources can be found at
https://github.com/theotherjimmy/autorandr-rs and bugs/patchs may be submitted
through the builtin github issue tracker.
