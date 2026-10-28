//! 강좌·회차·개인 진도.

mod model;
mod read;
mod write;

pub use model::{
    AddEpisodes, Course, CourseDetails, Episode, EpisodesAdded, MyProgress, NewEpisode,
    Participant, SetProgress, TodayQuery, UpdateEpisode,
};
pub use read::{__path_get, __path_participants, get, participants};
pub use write::{
    __path_add_episodes, __path_delete_episode, __path_set_details, __path_set_progress,
    __path_update_episode, add_episodes, delete_episode, set_details, set_progress, update_episode,
};
