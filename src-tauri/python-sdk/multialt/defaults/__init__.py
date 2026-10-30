"""Default blocks: the parts of MultiAlt's scripting you may rewrite.

App-owned folder. The files inside (``runner.py``, ``focus.py``, ``vision.py``,
``actions.py``, ``dashboard.py``, ``notify.py``) are *blocks*: with developer mode on you can
edit them in MultiAlt > Scripts > Roblox Player automation > Developer, and
restore any one of them to the original with "Restore default".

Whatever you change here, MultiAlt still enforces its safety rules itself
(the focus queue, input only to the window in front, tracked clients only,
no cookies or passwords), so a block can never bypass them.
"""
