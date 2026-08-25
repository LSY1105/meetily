-- Migration: Add MiniMax API Key to settings table
-- Adds support for the MiniMax (MiniMaxi open platform) summarization provider

ALTER TABLE settings ADD COLUMN minimaxApiKey TEXT;
