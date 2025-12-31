// @generated automatically by Diesel CLI.

diesel::table! {
    bno_sensor_data (id) {
        id -> Integer,
        timestamp -> Double,
        acc_x -> Integer,
        acc_y -> Integer,
        acc_z -> Integer,
        mag_x -> Integer,
        mag_y -> Integer,
        mag_z -> Integer,
        gyr_x -> Integer,
        gyr_y -> Integer,
        gyr_z -> Integer,
        eul_heading -> Integer,
        eul_roll -> Integer,
        eul_pitch -> Integer,
        qua_w -> Integer,
        qua_x -> Integer,
        qua_y -> Integer,
        qua_z -> Integer,
        lia_x -> Integer,
        lia_y -> Integer,
        lia_z -> Integer,
        grv_x -> Integer,
        grv_y -> Integer,
        grv_z -> Integer,
    }
}

diesel::table! {
    cpu_temperature (id) {
        id -> Integer,
        timestamp -> Double,
        degrees_celsius -> Double,
    }
}

diesel::table! {
    filesystem_usage (id) {
        id -> Integer,
        timestamp -> Double,
        free_mib -> Double,
        total_mib -> Double,
    }
}

diesel::table! {
    memory_usage (id) {
        id -> Integer,
        timestamp -> Double,
        available_mib -> Double,
        total_mib -> Double,
    }
}

diesel::table! {
    tel0157_readings (id) {
        id -> Integer,
        timestamp -> Double,
        latitude_degrees -> Double,
        longitude_degrees -> Double,
        course_over_ground_degrees -> Double,
        speed_over_ground_meters_per_second -> Double,
        altitude_meters -> Double,
        satellites -> Integer,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    bno_sensor_data,
    cpu_temperature,
    filesystem_usage,
    memory_usage,
    tel0157_readings,
);
