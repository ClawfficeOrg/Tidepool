-- Reverses 20261022000001_create_chatrooms.
--
-- Messages go before members and rooms: the reply self-reference and both
-- foreign keys must be gone before their parents can be dropped, which keeps
-- the file valid on a server that checks foreign keys strictly.

DROP TABLE IF EXISTS chat_messages;
DROP TABLE IF EXISTS chat_room_members;
DROP TABLE IF EXISTS chat_rooms;
