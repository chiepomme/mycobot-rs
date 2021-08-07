use super::common::*;
use super::io::Connection;
use byteorder::{BigEndian, ByteOrder};
use std::io;
use std::marker::PhantomData;

fn angle_to_int(degree: f64) -> i16 {
    (degree * 100.0) as i16
}

fn coord_to_int(coord: f64) -> i16 {
    (coord * 10.0) as i16
}

fn int_to_angle(val: i16) -> f64 {
    (val as f64) / 100.0
}

fn int_to_coord(val: i16) -> f64 {
    (val as f64) / 10.0
}

fn coords_to_int_vec(coords: &[f64]) -> Vec<i16> {
    coords
        .iter()
        .enumerate()
        .map(|(i, coord)| {
            if i < 3 {
                coord_to_int(*coord)
            } else {
                angle_to_int(*coord)
            }
        })
        .collect()
}

fn int_vec_to_coords(vals: &[i16]) -> Vec<f64> {
    vals.iter()
        .enumerate()
        .map(|(i, v)| {
            if i < 3 {
                int_to_coord(*v)
            } else {
                int_to_angle(*v)
            }
        })
        .collect()
}

pub struct MyCobotOperator<T: Connection> {
    connection: T,
    _marker: PhantomData<fn() -> T>,
}

impl<T: Connection> MyCobotOperator<T> {
    pub fn new(connection: T) -> MyCobotOperator<T> {
        MyCobotOperator {
            connection,
            _marker: PhantomData,
        }
    }
    fn encode_int16(data: i16) -> [u8; 2] {
        let mut buf = [0u8; 2];
        BigEndian::write_i16(&mut buf, data);
        buf
    }
    fn encode_int16_vec(data: &[i16]) -> Vec<u8> {
        let mut buf = Vec::<u8>::new();
        buf.resize(data.len() * 2, 0);
        for i in 0..data.len() {
            BigEndian::write_i16(&mut buf[(2 * i)..(2 * i + 2)], data[i]);
        }
        buf
    }
    fn decode_int16(data: &[u8]) -> i16 {
        BigEndian::read_i16(&data[0..2])
    }
    fn decode_int8(data: &[u8]) -> i8 {
        i8::from_be_bytes([data[0]])
    }
    fn decode_int16_vec(data: &[u8]) -> Vec<i16> {
        let mut res = Vec::<i16>::new();
        for idx in (0..(data.len())).step_by(2) {
            res.push(BigEndian::read_i16(&data[idx..(idx + 2)]));
        }
        res
    }
    fn concat_message(genre: u8, command_data: &[u8]) -> Vec<u8> {
        let len = 2 + command_data.len();
        let header = [Command::HEADER, Command::HEADER, len as u8, genre];
        [&header[..], command_data, &[Command::FOOTER]].concat()
    }
    fn is_frame_header(data: &[u8], pos: usize) -> bool {
        data[pos] == Command::HEADER && data[pos + 1] == Command::HEADER
    }
    fn process_received(data: &[u8], genre: u8) -> Vec<i16> {
        let some_idx =
            (0..(data.len() - 1)).position(|i| MyCobotOperator::<T>::is_frame_header(data, i));
        if let Some(idx) = some_idx {
            let data_len = (data[idx + 2] - 2) as usize;
            let cmd_id = data[idx + 3];
            if cmd_id != genre {
                Vec::<i16>::new()
            } else {
                let data_pos = idx + 4;
                let valid_data = &data[data_pos..(data_pos + data_len)];
                match data_len {
                    12 => MyCobotOperator::<T>::decode_int16_vec(valid_data),
                    2 => {
                        if genre == Command::IS_SERVO_ENABLE {
                            [MyCobotOperator::<T>::decode_int8(&valid_data[1..2]) as i16].to_vec()
                        } else {
                            [MyCobotOperator::<T>::decode_int16(valid_data)].to_vec()
                        }
                    }
                    _ => [MyCobotOperator::<T>::decode_int8(valid_data) as i16].to_vec(),
                }
            }
        } else {
            Vec::<i16>::new()
        }
    }
    pub fn version(&mut self) -> Result<String, io::Error> {
        let command = MyCobotOperator::<T>::concat_message(Command::VERSION, &Vec::<u8>::new());
        let res = self.connection.write_and_read(&command)?;
        let version = res.iter().map(|&s| s as char).collect::<String>();
        Ok(version)
    }
    pub fn power_on(&mut self) -> Result<(), io::Error> {
        let command = MyCobotOperator::<T>::concat_message(Command::POWER_ON, &Vec::<u8>::new());
        self.connection.write(&command)
    }
    pub fn power_off(&mut self) -> Result<(), io::Error> {
        let command = MyCobotOperator::<T>::concat_message(Command::POWER_OFF, &Vec::<u8>::new());
        self.connection.write(&command)
    }
    pub fn is_power_on(&mut self) -> Result<i32, io::Error> {
        let command = MyCobotOperator::<T>::concat_message(Command::IS_POWER_ON, &Vec::<u8>::new());
        let res = self.connection.write_and_read(&command)?;
        Ok(if res.is_empty() { -1 } else { res[0] as i32 })
    }
    pub fn release_all_servos(&mut self) -> Result<(), io::Error> {
        let command =
            MyCobotOperator::<T>::concat_message(Command::RELEASE_ALL_SERVOS, &Vec::<u8>::new());
        self.connection.write(&command)
    }
    pub fn is_controller_connected(&mut self) -> Result<i32, io::Error> {
        let command = MyCobotOperator::<T>::concat_message(
            Command::IS_CONTROLLER_CONNECTED,
            &Vec::<u8>::new(),
        );
        let res = self.connection.write_and_read(&command)?;
        Ok(if res.is_empty() { -1 } else { res[0] as i32 })
    }
    pub fn get_angles(&mut self) -> Result<Vec<f64>, io::Error> {
        let command = MyCobotOperator::<T>::concat_message(Command::GET_ANGLES, &Vec::<u8>::new());
        let res = self.connection.write_and_read(&command)?;
        let res = MyCobotOperator::<T>::process_received(&res, Command::GET_ANGLES);
        Ok(res.into_iter().map(int_to_angle).collect::<Vec<_>>())
    }
    pub fn send_angle(&mut self, id: Angle, degree: f64, speed: u8) -> Result<(), io::Error> {
        let command_data = [
            &[id as u8],
            &MyCobotOperator::<T>::encode_int16(angle_to_int(degree))[..],
            &[speed],
        ]
        .concat();
        let command = MyCobotOperator::<T>::concat_message(Command::SEND_ANGLE, &command_data);
        self.connection.write(&command)
    }
    pub fn send_angles(&mut self, degrees: &[f64], speed: u8) -> Result<(), io::Error> {
        let command_data = [
            &MyCobotOperator::<T>::encode_int16_vec(
                &degrees
                    .iter()
                    .map(|deg| angle_to_int(*deg))
                    .collect::<Vec<_>>()[..],
            )[..],
            &[speed],
        ]
        .concat();
        let command = MyCobotOperator::<T>::concat_message(Command::SEND_ANGLES, &command_data);
        self.connection.write(&command)
    }
    pub fn get_coords(&mut self) -> Result<Vec<f64>, io::Error> {
        let command = MyCobotOperator::<T>::concat_message(Command::GET_COORDS, &Vec::<u8>::new());
        let res = self.connection.write_and_read(&command)?;
        let res = MyCobotOperator::<T>::process_received(&res, Command::GET_COORDS);
        Ok(int_vec_to_coords(&res))
    }
    pub fn send_coord(&mut self, id: Coord, coord: f64, speed: u8) -> Result<(), io::Error> {
        let command_data = [
            &[id as u8 - 1],
            &MyCobotOperator::<T>::encode_int16(coord_to_int(coord))[..],
            &[speed],
        ]
        .concat();
        let command = MyCobotOperator::<T>::concat_message(Command::SEND_COORD, &command_data);
        self.connection.write(&command)
    }
    pub fn send_coords(&mut self, coords: &[f64], speed: u8, mode: u8) -> Result<(), io::Error> {
        let command_data = [
            &MyCobotOperator::<T>::encode_int16_vec(&coords_to_int_vec(coords))[..],
            &[speed],
            &[mode],
        ]
        .concat();
        let command = MyCobotOperator::<T>::concat_message(Command::SEND_COORDS, &command_data);
        self.connection.write(&command)
    }
    pub fn is_in_angle_position(&mut self, degrees: &[f64; 6]) -> Result<i32, io::Error> {
        let command_data = [
            &MyCobotOperator::<T>::encode_int16_vec(
                &degrees
                    .iter()
                    .map(|deg| angle_to_int(*deg))
                    .collect::<Vec<_>>()[..],
            )[..],
            &[0u8],
        ]
        .concat();
        let command = MyCobotOperator::<T>::concat_message(Command::IS_IN_POSITION, &command_data);
        let res = self.connection.write_and_read(&command)?;
        Ok(if res.is_empty() { -1 } else { res[0] as i32 })
    }
    pub fn is_in_coord_position(&mut self, coords: &[f64]) -> Result<i32, io::Error> {
        let command_data = [
            &MyCobotOperator::<T>::encode_int16_vec(&coords_to_int_vec(coords))[..],
            &[1u8],
        ]
        .concat();
        let command = MyCobotOperator::<T>::concat_message(Command::IS_IN_POSITION, &command_data);
        let res = self.connection.write_and_read(&command)?;
        Ok(if res.is_empty() { -1 } else { res[0] as i32 })
    }
    pub fn is_moving(&mut self) -> Result<i32, io::Error> {
        let command = MyCobotOperator::<T>::concat_message(Command::IS_MOVING, &Vec::<u8>::new());
        let res = self.connection.write_and_read(&command)?;
        Ok(if res.is_empty() { -1 } else { res[0] as i32 })
    }
    pub fn jog_angle(
        &mut self,
        id: Angle,
        direction: Direction,
        speed: u8,
    ) -> Result<(), io::Error> {
        let command_data = [id as u8, direction as u8, speed];
        let command =
            MyCobotOperator::<T>::concat_message(Command::JOG_ANGLE, &command_data.to_vec());
        self.connection.write(&command)
    }
    pub fn jog_coord(
        &mut self,
        id: Coord,
        direction: Direction,
        speed: u8,
    ) -> Result<(), io::Error> {
        let command_data = [id as u8, direction as u8, speed];
        let command =
            MyCobotOperator::<T>::concat_message(Command::JOG_COORD, &command_data.to_vec());
        self.connection.write(&command)
    }
    pub fn jog_stop(&mut self) -> Result<(), io::Error> {
        let command = MyCobotOperator::<T>::concat_message(Command::JOG_STOP, &Vec::<u8>::new());
        self.connection.write(&command)
    }
    pub fn pause(&mut self) -> Result<(), io::Error> {
        let command = MyCobotOperator::<T>::concat_message(Command::PAUSE, &Vec::<u8>::new());
        self.connection.write(&command)
    }
    pub fn is_paused(&mut self) -> Result<i32, io::Error> {
        let command = MyCobotOperator::<T>::concat_message(Command::IS_PAUSED, &Vec::<u8>::new());
        let res = self.connection.write_and_read(&command)?;
        Ok(if res.is_empty() { -1 } else { res[0] as i32 })
    }
    pub fn resume(&mut self) -> Result<(), io::Error> {
        let command = MyCobotOperator::<T>::concat_message(Command::RESUME, &Vec::<u8>::new());
        self.connection.write(&command)
    }
    pub fn stop(&mut self) -> Result<(), io::Error> {
        let command = MyCobotOperator::<T>::concat_message(Command::STOP, &Vec::<u8>::new());
        self.connection.write(&command)
    }
    pub fn set_encoder(&mut self, id: Angle, encoder: i16) -> Result<(), io::Error> {
        let command_data = [
            &[id as u8],
            &MyCobotOperator::<T>::encode_int16(encoder)[..],
        ]
        .concat();
        let command = MyCobotOperator::<T>::concat_message(Command::SET_ENCODER, &command_data);
        self.connection.write(&command)
    }
    pub fn get_encoder(&mut self, id: Angle) -> Result<i32, io::Error> {
        let command_data = [id as u8];
        let command =
            MyCobotOperator::<T>::concat_message(Command::GET_ENCODER, &command_data.to_vec());
        let res = self.connection.write_and_read(&command)?;
        Ok(if res.is_empty() { -1 } else { res[0] as i32 })
    }
}