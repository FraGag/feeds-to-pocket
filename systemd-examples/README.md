# systemd example units

If you would like to use systemd as a scheduler for <b>Feeds to Pocket</b>,
you can use the unit files in this directory as a basis.
You will most likely want to setup this for your user instance,
rather than for the system instance,
so copy `feeds-to-pocket.service` to `~/.config/systemd/user`
and copy `feeds-to-pocket.timer` to `~/.config/systemd/user/timers.target.wants`.

In `feeds-to-pocket.service`,
edit the `ExecStart` option in the `[Service]` section
to refer to your own configuration file.

In `feeds-to-pocket.timer`,
you may configure the frequency
at which <b>Feeds to Pocket</b> will run.
The provided timer will run 1 minute after login
and every hour after that.

Once you've configured the unit files to your liking,
start the timer with the following command:

    $ systemctl --user start feeds-to-pocket.timer

You can also run <b>Feeds to Pocket</b> at any time
with the following command:

    $ systemctl --user start feeds-to-pocket.service

systemd will capture the <b>Feeds To Pocket</b>'s output
and save it in its journal.
You can view the captured output
with this command:

    $ journalctl --user-unit feeds-to-pocket.service
